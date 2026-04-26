mod fbx_import;
// File I/O actors require tokio::fs, which is native-only.
// On wasm targets, fetch + IndexedDB are the equivalents — see
// reflow_assets and the network/integration actors.
#[cfg(not(target_arch = "wasm32"))]
mod file_load;
#[cfg(not(target_arch = "wasm32"))]
mod file_save;
mod gltf_export;
pub(crate) mod gltf_import;
mod mesh_import;
mod obj_export;
mod obj_import;
mod scene_import;
mod stl_export;
pub(crate) mod stl_import;

pub use fbx_import::FbxImportActor;
#[cfg(not(target_arch = "wasm32"))]
pub use file_load::FileLoadActor;
#[cfg(not(target_arch = "wasm32"))]
pub use file_save::FileSaveActor;
pub use gltf_export::GltfExportActor;
pub use gltf_import::GltfImportActor;
pub use mesh_import::MeshImportActor;
pub use obj_export::ObjExportActor;
pub use obj_import::ObjImportActor;
pub use scene_import::SceneImportActor;
pub use stl_export::StlExportActor;
pub use stl_import::StlImportActor;
