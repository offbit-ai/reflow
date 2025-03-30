// use undo::Record;
// #[cfg(target_arch = "wasm32")]
// use wasm_bindgen::prelude::*;

// use tsify::Tsify;
// use serde::Serialize;
// use serde::Deserialize;
// use super::GraphEvents;
// use super::{types::GraphExport, Graph};

// /// Graph Journal store to track changes
// #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
// #[derive(Clone, Serialize, Deserialize)]
// pub struct JournalStore(pub GraphEvents);


// impl undo::Edit for JournalStore {
//     type Target = GraphEvents;

//     type Output = Graph;

//     fn edit(&mut self, target: &mut Self::Target) -> Self::Output {
//         // target.push(self.0.clone());
//         match target {
//             GraphEvents::AddNode(value) => todo!(),
//             GraphEvents::RemoveNode(value) => todo!(),
//             GraphEvents::RenameNode(value) => todo!(),
//             GraphEvents::ChangeNode(value) => todo!(),
//             GraphEvents::AddConnection(value) => todo!(),
//             GraphEvents::RemoveConnection(value) => todo!(),
//             GraphEvents::ChangeConnection(value) => todo!(),
//             GraphEvents::AddInitial(value) => todo!(),
//             GraphEvents::RemoveInitial(value) => todo!(),
//             GraphEvents::ChangeProperties(value) => todo!(),
//             GraphEvents::AddGroup(value) => todo!(),
//             GraphEvents::RemoveGroup(value) => todo!(),
//             GraphEvents::RenameGroup(value) => todo!(),
//             GraphEvents::ChangeGroup(value) => todo!(),
//             GraphEvents::AddInport(value) => todo!(),
//             GraphEvents::RemoveInport(value) => todo!(),
//             GraphEvents::RenameInport(value) => todo!(),
//             GraphEvents::ChangeInport(value) => todo!(),
//             GraphEvents::AddOutport(value) => todo!(),
//             GraphEvents::RemoveOutport(value) => todo!(),
//             GraphEvents::RenameOutport(value) => todo!(),
//             GraphEvents::ChangeOutport(value) => todo!(),
//             GraphEvents::StartTransaction(value) => todo!(),
//             GraphEvents::EndTransaction(value) => todo!(),
//             GraphEvents::Transaction(value) => todo!(),
//             GraphEvents::None => todo!(),
//         }
//     }

//     fn undo(&mut self, target: &mut Self::Target) -> Self::Output {
//         match target {
//             GraphEvents::AddNode(value) => todo!(),
//             GraphEvents::RemoveNode(value) => todo!(),
//             GraphEvents::RenameNode(value) => todo!(),
//             GraphEvents::ChangeNode(value) => todo!(),
//             GraphEvents::AddConnection(value) => todo!(),
//             GraphEvents::RemoveConnection(value) => todo!(),
//             GraphEvents::ChangeConnection(value) => todo!(),
//             GraphEvents::AddInitial(value) => todo!(),
//             GraphEvents::RemoveInitial(value) => todo!(),
//             GraphEvents::ChangeProperties(value) => todo!(),
//             GraphEvents::AddGroup(value) => todo!(),
//             GraphEvents::RemoveGroup(value) => todo!(),
//             GraphEvents::RenameGroup(value) => todo!(),
//             GraphEvents::ChangeGroup(value) => todo!(),
//             GraphEvents::AddInport(value) => todo!(),
//             GraphEvents::RemoveInport(value) => todo!(),
//             GraphEvents::RenameInport(value) => todo!(),
//             GraphEvents::ChangeInport(value) => todo!(),
//             GraphEvents::AddOutport(value) => todo!(),
//             GraphEvents::RemoveOutport(value) => todo!(),
//             GraphEvents::RenameOutport(value) => todo!(),
//             GraphEvents::ChangeOutport(value) => todo!(),
//             GraphEvents::StartTransaction(value) => todo!(),
//             GraphEvents::EndTransaction(value) => todo!(),
//             GraphEvents::Transaction(value) => todo!(),
//             GraphEvents::None => todo!(),
//         }
//     }
// }



// #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
// pub struct Journal {
//     // pub store: JournalStore,
//     pub (crate) rec: Record<JournalStore>
// }

// #[cfg_attr(target_arch = "wasm32", wasm_bindgen)]
// impl Journal  {
//     #[wasm_bindgen(js_class = GraphJournal)]
//     pub fn _new(graph:Graph) -> Self {
//         Self { rec: Record::new()}
//     }

//     pub fn undo(&mut self) {
//         self.rec.undo(&mut self.store);
//         // Graph::load(self.store.0, None)
//     }
// }
