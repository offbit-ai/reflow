use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::io::{self, ErrorKind};
use std::time::{SystemTime, UNIX_EPOCH};

/// File metadata
#[derive(Debug, Clone)]
pub struct FileMetadata {
    /// File size in bytes
    pub size: usize,
    
    /// Creation time
    pub created: SystemTime,
    
    /// Last modified time
    pub modified: SystemTime,
    
    /// Last accessed time
    pub accessed: SystemTime,
    
    /// Is directory
    pub is_dir: bool,
}

impl FileMetadata {
    /// Create new file metadata
    pub fn new(size: usize, is_dir: bool) -> Self {
        let now = SystemTime::now();
        
        FileMetadata {
            size,
            created: now,
            modified: now,
            accessed: now,
            is_dir,
        }
    }
    
    /// Update the file size
    pub fn update_size(&mut self, size: usize) {
        self.size = size;
        self.modified = SystemTime::now();
        self.accessed = SystemTime::now();
    }
    
    /// Update the access time
    pub fn update_access(&mut self) {
        self.accessed = SystemTime::now();
    }
}

/// Virtual file system
#[derive(Debug)]
pub struct VirtualFileSystem {
    /// Files
    files: HashMap<PathBuf, Vec<u8>>,
    
    /// Directories
    directories: HashMap<PathBuf, Vec<PathBuf>>,
    
    /// Metadata
    metadata: HashMap<PathBuf, FileMetadata>,
}

impl VirtualFileSystem {
    /// Create a new virtual file system
    pub fn new() -> Self {
        let mut vfs = VirtualFileSystem {
            files: HashMap::new(),
            directories: HashMap::new(),
            metadata: HashMap::new(),
        };
        
        // Create root directory
        vfs.directories.insert(PathBuf::from("/"), Vec::new());
        vfs.metadata.insert(PathBuf::from("/"), FileMetadata::new(0, true));
        
        vfs
    }
    
    /// Normalize a path
    fn normalize_path(&self, path: &Path) -> PathBuf {
        let path_str = path.to_string_lossy().to_string();
        let path_str = path_str.replace("\\", "/");
        
        PathBuf::from(path_str)
    }
    
    /// Get the parent directory of a path
    fn parent_dir(&self, path: &Path) -> Option<PathBuf> {
        let path = self.normalize_path(path);
        path.parent().map(|p| self.normalize_path(p))
    }
    
    /// Check if a path exists
    pub fn exists(&self, path: &Path) -> bool {
        let path = self.normalize_path(path);
        self.files.contains_key(&path) || self.directories.contains_key(&path)
    }
    
    /// Check if a path is a directory
    pub fn is_dir(&self, path: &Path) -> bool {
        let path = self.normalize_path(path);
        self.directories.contains_key(&path)
    }
    
    /// Check if a path is a file
    pub fn is_file(&self, path: &Path) -> bool {
        let path = self.normalize_path(path);
        self.files.contains_key(&path)
    }
    
    /// Get file metadata
    pub fn metadata(&self, path: &Path) -> io::Result<FileMetadata> {
        let path = self.normalize_path(path);
        
        if let Some(metadata) = self.metadata.get(&path) {
            Ok(metadata.clone())
        } else {
            Err(io::Error::new(ErrorKind::NotFound, "File not found"))
        }
    }
    
    /// Create a directory
    pub fn create_dir(&mut self, path: &Path) -> io::Result<()> {
        let path = self.normalize_path(path);
        
        // Check if the directory already exists
        if self.directories.contains_key(&path) {
            return Err(io::Error::new(ErrorKind::AlreadyExists, "Directory already exists"));
        }
        
        // Check if the parent directory exists
        if let Some(parent) = self.parent_dir(&path) {
            if !self.directories.contains_key(&parent) {
                return Err(io::Error::new(ErrorKind::NotFound, "Parent directory not found"));
            }
            
            // Add the directory to the parent
            if let Some(parent_entries) = self.directories.get_mut(&parent) {
                parent_entries.push(path.clone());
            }
        }
        
        // Create the directory
        self.directories.insert(path.clone(), Vec::new());
        self.metadata.insert(path, FileMetadata::new(0, true));
        
        Ok(())
    }
    
    /// Create a directory and all parent directories
    pub fn create_dir_all(&mut self, path: &Path) -> io::Result<()> {
        let path = self.normalize_path(path);
        
        // Check if the directory already exists
        if self.directories.contains_key(&path) {
            return Ok(());
        }
        
        // Create parent directories
        if let Some(parent) = self.parent_dir(&path) {
            if !self.directories.contains_key(&parent) {
                self.create_dir_all(&parent)?;
            }
        }
        
        // Create the directory
        self.create_dir(&path)?;
        
        Ok(())
    }
    
    /// Remove a directory
    pub fn remove_dir(&mut self, path: &Path) -> io::Result<()> {
        let path = self.normalize_path(path);
        
        // Check if the directory exists
        if !self.directories.contains_key(&path) {
            return Err(io::Error::new(ErrorKind::NotFound, "Directory not found"));
        }
        
        // Check if the directory is empty
        if let Some(entries) = self.directories.get(&path) {
            if !entries.is_empty() {
                return Err(io::Error::new(ErrorKind::Other, "Directory not empty"));
            }
        }
        
        // Remove the directory from the parent
        if let Some(parent) = self.parent_dir(&path) {
            if let Some(parent_entries) = self.directories.get_mut(&parent) {
                parent_entries.retain(|p| p != &path);
            }
        }
        
        // Remove the directory
        self.directories.remove(&path);
        self.metadata.remove(&path);
        
        Ok(())
    }
    
    /// Remove a directory and all its contents
    pub fn remove_dir_all(&mut self, path: &Path) -> io::Result<()> {
        let path = self.normalize_path(path);
        
        // Check if the directory exists
        if !self.directories.contains_key(&path) {
            return Err(io::Error::new(ErrorKind::NotFound, "Directory not found"));
        }
        
        // Get all entries in the directory
        let entries = if let Some(entries) = self.directories.get(&path) {
            entries.clone()
        } else {
            Vec::new()
        };
        
        // Remove all entries
        for entry in entries {
            if self.is_dir(&entry) {
                self.remove_dir_all(&entry)?;
            } else {
                self.remove_file(&entry)?;
            }
        }
        
        // Remove the directory
        self.remove_dir(&path)?;
        
        Ok(())
    }
    
    /// Read a directory
    pub fn read_dir(&self, path: &Path) -> io::Result<Vec<String>> {
        let path = self.normalize_path(path);
        
        // Check if the directory exists
        if !self.directories.contains_key(&path) {
            return Err(io::Error::new(ErrorKind::NotFound, "Directory not found"));
        }
        
        // Get all entries in the directory
        let entries = if let Some(entries) = self.directories.get(&path) {
            entries.clone()
        } else {
            Vec::new()
        };
        
        // Convert entries to strings
        let mut result = Vec::new();
        for entry in entries {
            if let Some(file_name) = entry.file_name() {
                result.push(file_name.to_string_lossy().to_string());
            }
        }
        
        Ok(result)
    }
    
    /// Write a file
    pub fn write_file(&mut self, path: &Path, data: &[u8]) -> io::Result<()> {
        let path = self.normalize_path(path);
        
        // Create parent directories if they don't exist
        if let Some(parent) = self.parent_dir(&path) {
            if !self.directories.contains_key(&parent) {
                self.create_dir_all(&parent)?;
            }
        }
        
        // Add the file to the parent directory
        if let Some(parent) = self.parent_dir(&path) {
            if let Some(parent_entries) = self.directories.get_mut(&parent) {
                if !parent_entries.contains(&path) {
                    parent_entries.push(path.clone());
                }
            }
        }
        
        // Write the file
        self.files.insert(path.clone(), data.to_vec());
        
        // Update metadata
        if let Some(metadata) = self.metadata.get_mut(&path) {
            metadata.update_size(data.len());
        } else {
            self.metadata.insert(path, FileMetadata::new(data.len(), false));
        }
        
        Ok(())
    }
    
    /// Read a file
    pub fn read_file(&self, path: &Path) -> io::Result<Vec<u8>> {
        let path = self.normalize_path(path);
        
        // Check if the file exists
        if !self.files.contains_key(&path) {
            return Err(io::Error::new(ErrorKind::NotFound, "File not found"));
        }
        
        // Read the file
        let data = self.files.get(&path).unwrap().clone();
        
        // Update access time
        if let Some(metadata) = self.metadata.get_mut(&path) {
            metadata.update_access();
        }
        
        Ok(data)
    }
    
    /// Remove a file
    pub fn remove_file(&mut self, path: &Path) -> io::Result<()> {
        let path = self.normalize_path(path);
        
        // Check if the file exists
        if !self.files.contains_key(&path) {
            return Err(io::Error::new(ErrorKind::NotFound, "File not found"));
        }
        
        // Remove the file from the parent directory
        if let Some(parent) = self.parent_dir(&path) {
            if let Some(parent_entries) = self.directories.get_mut(&parent) {
                parent_entries.retain(|p| p != &path);
            }
        }
        
        // Remove the file
        self.files.remove(&path);
        self.metadata.remove(&path);
        
        Ok(())
    }
    
    /// Rename a file or directory
    pub fn rename(&mut self, from: &Path, to: &Path) -> io::Result<()> {
        let from = self.normalize_path(from);
        let to = self.normalize_path(to);
        
        // Check if the source exists
        if !self.exists(&from) {
            return Err(io::Error::new(ErrorKind::NotFound, "Source not found"));
        }
        
        // Check if the destination already exists
        if self.exists(&to) {
            return Err(io::Error::new(ErrorKind::AlreadyExists, "Destination already exists"));
        }
        
        // Create parent directories if they don't exist
        if let Some(parent) = self.parent_dir(&to) {
            if !self.directories.contains_key(&parent) {
                self.create_dir_all(&parent)?;
            }
        }
        
        // Remove the source from the parent directory
        if let Some(parent) = self.parent_dir(&from) {
            if let Some(parent_entries) = self.directories.get_mut(&parent) {
                parent_entries.retain(|p| p != &from);
            }
        }
        
        // Add the destination to the parent directory
        if let Some(parent) = self.parent_dir(&to) {
            if let Some(parent_entries) = self.directories.get_mut(&parent) {
                parent_entries.push(to.clone());
            }
        }
        
        // Move the file or directory
        if self.is_file(&from) {
            let data = self.files.remove(&from).unwrap();
            self.files.insert(to.clone(), data);
        } else {
            let entries = self.directories.remove(&from).unwrap();
            self.directories.insert(to.clone(), entries);
        }
        
        // Move the metadata
        if let Some(metadata) = self.metadata.remove(&from) {
            self.metadata.insert(to, metadata);
        }
        
        Ok(())
    }
}

impl Default for VirtualFileSystem {
    fn default() -> Self {
        Self::new()
    }
}
