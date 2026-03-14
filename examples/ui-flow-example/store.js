import {createStore} from "./zustand/vanilla.js";


export const EditorState = createStore(()=>({
    drawerIsOpen: false,
    drawerResized: false,
    toolbarCollapsed: false
}));