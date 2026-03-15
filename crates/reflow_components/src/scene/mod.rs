//! Scene composition actors: prefabs, instances, scene graph, terrain.

mod instance;
mod prefab;
mod scene_graph;
mod terrain;

pub use instance::InstanceActor;
pub use prefab::PrefabActor;
pub use scene_graph::SceneGraphActor;
pub use terrain::TerrainActor;
