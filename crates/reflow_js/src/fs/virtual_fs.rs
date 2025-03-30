use crate::security::PermissionManager;
use boa_engine::builtins::function::Function;
use boa_engine::js_string;
use boa_engine::{
    native_function::NativeFunction, object::ObjectInitializer, Context, JsError, JsResult, JsValue,
};
use std::collections::HashMap;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

/// File metadata
#[derive(Debug, Clone)]
pub struct FileMetadata {
    /// File size
    pub size: usize,

    /// Is directory
    pub is_dir: bool,

    /// Creation time
    pub created: SystemTime,

    /// Last modified time
    pub modified: SystemTime,

    /// Last accessed time
    pub accessed: SystemTime,
}

impl Default for FileMetadata {
    fn default() -> Self {
        let now = SystemTime::now();
        FileMetadata {
            size: 0,
            is_dir: false,
            created: now,
            modified: now,
            accessed: now,
        }
    }
}

/// Virtual file system
pub struct VirtualFileSystem {
    /// Permission manager
    permissions: PermissionManager,

    /// Files
    files: Arc<Mutex<HashMap<PathBuf, Vec<u8>>>>,

    /// Directories
    directories: Arc<Mutex<HashMap<PathBuf, Vec<PathBuf>>>>,

    /// Metadata
    metadata: Arc<Mutex<HashMap<PathBuf, FileMetadata>>>,
}

// Global storage for file system data
static mut FILES: Option<Arc<Mutex<HashMap<PathBuf, Vec<u8>>>>> = None;
static mut DIRECTORIES: Option<Arc<Mutex<HashMap<PathBuf, Vec<PathBuf>>>>> = None;
static mut METADATA: Option<Arc<Mutex<HashMap<PathBuf, FileMetadata>>>> = None;

// Function pointers for file system operations
fn read_file_sync(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    // Check arguments
    if args.is_empty() {
        return Err(JsError::from_opaque(
            "readFileSync requires a path argument".into(),
        ));
    }

    // Get the path
    let path = args[0].to_string(ctx)?.to_std_string_escaped();

    // Get the files
    let files = unsafe { FILES.as_ref().unwrap().lock().unwrap() };

    // Check if the file exists
    let path_buf = PathBuf::from(path);
    if let Some(content) = files.get(&path_buf) {
        // Return the content
        let content_str = String::from_utf8_lossy(content).to_string();
        Ok(JsValue::from(content_str))
    } else {
        // Return an error
        Err(JsError::from_opaque("File not found".into()))
    }
}

fn write_file_sync(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    // Check arguments
    if args.len() < 2 {
        return Err(JsError::from_opaque(
            "writeFileSync requires path and data arguments".into(),
        ));
    }

    // Get the path
    let path = args[0].to_string(ctx)?.to_std_string_escaped();

    // Get the data
    let data = args[1].to_string(ctx)?.to_std_string_escaped();

    // Get the files and metadata
    let mut files = unsafe { FILES.as_ref().unwrap().lock().unwrap() };
    let mut metadata = unsafe { METADATA.as_ref().unwrap().lock().unwrap() };

    // Write the file
    let path_buf = PathBuf::from(path);
    files.insert(path_buf.clone(), data.as_bytes().to_vec());

    // Update the metadata
    let mut file_metadata = FileMetadata::default();
    file_metadata.size = data.len();
    file_metadata.modified = SystemTime::now();
    metadata.insert(path_buf, file_metadata);

    // Return undefined
    Ok(JsValue::undefined())
}

fn exists_sync(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    // Check arguments
    if args.is_empty() {
        return Err(JsError::from_opaque(
            "existsSync requires a path argument".into(),
        ));
    }

    // Get the path
    let path = args[0].to_string(ctx)?.to_std_string_escaped();

    // Get the files and directories
    let files = unsafe { FILES.as_ref().unwrap().lock().unwrap() };
    let directories = unsafe { DIRECTORIES.as_ref().unwrap().lock().unwrap() };

    // Check if the file or directory exists
    let path_buf = PathBuf::from(path);
    let exists = files.contains_key(&path_buf) || directories.contains_key(&path_buf);

    // Return the result
    Ok(JsValue::from(exists))
}

fn mkdir_sync(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    // Check arguments
    if args.is_empty() {
        return Err(JsError::from_opaque(
            "mkdirSync requires a path argument".into(),
        ));
    }

    // Get the path
    let path = args[0].to_string(ctx)?.to_std_string_escaped();

    // Get the directories and metadata
    let mut directories = unsafe { DIRECTORIES.as_ref().unwrap().lock().unwrap() };
    let mut metadata = unsafe { METADATA.as_ref().unwrap().lock().unwrap() };

    // Create the directory
    let path_buf = PathBuf::from(path);
    directories.insert(path_buf.clone(), Vec::new());

    // Update the metadata
    let mut dir_metadata = FileMetadata::default();
    dir_metadata.is_dir = true;
    metadata.insert(path_buf, dir_metadata);

    // Return undefined
    Ok(JsValue::undefined())
}

fn rmdir_sync(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    // Check arguments
    if args.is_empty() {
        return Err(JsError::from_opaque(
            "rmdirSync requires a path argument".into(),
        ));
    }

    // Get the path
    let path = args[0].to_string(ctx)?.to_std_string_escaped();

    // Get the directories and metadata
    let mut directories = unsafe { DIRECTORIES.as_ref().unwrap().lock().unwrap() };
    let mut metadata = unsafe { METADATA.as_ref().unwrap().lock().unwrap() };

    // Remove the directory
    let path_buf = PathBuf::from(path);
    if let Some(entries) = directories.get(&path_buf) {
        if !entries.is_empty() {
            return Err(JsError::from_opaque("Directory not empty".into()));
        }

        directories.remove(&path_buf);
        metadata.remove(&path_buf);

        // Return undefined
        Ok(JsValue::undefined())
    } else {
        // Return an error
        Err(JsError::from_opaque("Directory not found".into()))
    }
}

fn unlink_sync(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    // Check arguments
    if args.is_empty() {
        return Err(JsError::from_opaque(
            "unlinkSync requires a path argument".into(),
        ));
    }

    // Get the path
    let path = args[0].to_string(ctx)?.to_std_string_escaped();

    // Get the files and metadata
    let mut files = unsafe { FILES.as_ref().unwrap().lock().unwrap() };
    let mut metadata = unsafe { METADATA.as_ref().unwrap().lock().unwrap() };

    // Remove the file
    let path_buf = PathBuf::from(path);
    if files.contains_key(&path_buf) {
        files.remove(&path_buf);
        metadata.remove(&path_buf);

        // Return undefined
        Ok(JsValue::undefined())
    } else {
        // Return an error
        Err(JsError::from_opaque("File not found".into()))
    }
}

fn stat_sync(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    // Check arguments
    if args.is_empty() {
        return Err(JsError::from_opaque(
            "statSync requires a path argument".into(),
        ));
    }

    // Get the path
    let path = args[0].to_string(ctx)?.to_std_string_escaped();

    // Get the files, directories, and metadata
    let files = unsafe { FILES.as_ref().unwrap().lock().unwrap() };
    let directories = unsafe { DIRECTORIES.as_ref().unwrap().lock().unwrap() };
    let metadata = unsafe { METADATA.as_ref().unwrap().lock().unwrap() };

    // Get the file or directory metadata
    let path_buf = PathBuf::from(path);
    if let Some(meta) = metadata.get(&path_buf) {
        // Create a stats object
        let stats_obj = ObjectInitializer::new(ctx).build();

        // Add the stats properties
        stats_obj.set("size", meta.size as u32, true, ctx)?;
        stats_obj.set("isFile", !meta.is_dir, true, ctx)?;
        stats_obj.set("isDirectory", meta.is_dir, true, ctx)?;

        // Return the stats object
        Ok(stats_obj.into())
    } else if files.contains_key(&path_buf) || directories.contains_key(&path_buf) {
        // Create a stats object with default values
        let stats_obj = ObjectInitializer::new(ctx).build();

        // Add the stats properties
        let is_dir = directories.contains_key(&path_buf);
        stats_obj.set("size", 0, true, ctx)?;
        stats_obj.set("isFile", !is_dir, true, ctx)?;
        stats_obj.set("isDirectory", is_dir, true, ctx)?;

        // Return the stats object
        Ok(stats_obj.into())
    } else {
        // Return an error
        Err(JsError::from_opaque("File or directory not found".into()))
    }
}

fn read_dir_sync(_this: &JsValue, args: &[JsValue], ctx: &mut Context) -> JsResult<JsValue> {
    // Check arguments
    if args.is_empty() {
        return Err(JsError::from_opaque(
            "readdirSync requires a path argument".into(),
        ));
    }

    // Get the path
    let path = args[0].to_string(ctx)?.to_std_string_escaped();

    // Get the directories
    let directories = unsafe { DIRECTORIES.as_ref().unwrap().lock().unwrap() };

    // Get the directory entries
    let path_buf = PathBuf::from(path);
    if let Some(entries) = directories.get(&path_buf) {
        // Create an array to hold the entries
        let entries_obj = ObjectInitializer::new(ctx).build();

        // Add the entries to the array
        for (i, entry) in entries.iter().enumerate() {
            // Get the file name
            let file_name = entry.file_name().unwrap().to_string_lossy().to_string();

            // Add the file name to the array
            entries_obj.set(i as u32, file_name, true, ctx)?;
        }

        // Return the array
        Ok(entries_obj.into())
    } else {
        // Return an error
        Err(JsError::from_opaque("Directory not found".into()))
    }
}

impl VirtualFileSystem {
    /// Create a new virtual file system
    pub fn new(permissions: PermissionManager) -> Self {
        // Create the root directory
        let mut directories = HashMap::new();
        directories.insert(PathBuf::from("/"), Vec::new());

        // Create the root directory metadata
        let mut metadata = HashMap::new();
        let mut root_metadata = FileMetadata::default();
        root_metadata.is_dir = true;
        metadata.insert(PathBuf::from("/"), root_metadata);

        let files = Arc::new(Mutex::new(HashMap::new()));
        let directories = Arc::new(Mutex::new(directories));
        let metadata = Arc::new(Mutex::new(metadata));

        // Store the file system data in the global storage
        unsafe {
            FILES = Some(files.clone());
            DIRECTORIES = Some(directories.clone());
            METADATA = Some(metadata.clone());
        }

        VirtualFileSystem {
            permissions,
            files,
            directories,
            metadata,
        }
    }

    /// Register the virtual file system with a JavaScript context
    pub fn register(&self, context: &mut Context) -> JsResult<()> {
        // Create the vfs object
        let vfs_obj = ObjectInitializer::new(context)
            .function(
                NativeFunction::from_fn_ptr(read_file_sync),
                js_string!("readFileSync"),
                1,
            )
            .function(
                NativeFunction::from_fn_ptr(write_file_sync),
                js_string!("writeFileSync"),
                2,
            )
            .function(
                NativeFunction::from_fn_ptr(exists_sync),
                js_string!("existsSync"),
                1,
            )
            .function(
                NativeFunction::from_fn_ptr(mkdir_sync),
                js_string!("mkdirSync"),
                1,
            )
            .function(
                NativeFunction::from_fn_ptr(rmdir_sync),
                js_string!("rmdirSync"),
                1,
            )
            .function(
                NativeFunction::from_fn_ptr(unlink_sync),
                js_string!("unlinkSync"),
                1,
            )
            .function(
                NativeFunction::from_fn_ptr(stat_sync),
                js_string!("statSync"),
                1,
            )
            .function(
                NativeFunction::from_fn_ptr(read_dir_sync),
                js_string!("readdirSync"),
                1,
            )
            .build();

       

        // Add the vfs object to the global object
        let global = context.global_object();
        global.set("vfs", vfs_obj, true, context)?;

        Ok(())
    }
}
