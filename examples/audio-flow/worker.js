import { Graph, GraphHistory, StorageManager, initSync} from "../../crates/reflow_network/pkg/reflow_network.js";


fetch("../../crates/reflow_network/pkg/reflow_network_bg.wasm").then(async (res) => {
    initSync(await res.arrayBuffer());
    self.postMessage("READY");
});

/**
    * @type {Graph}
    */
let graph = null;

/**
 * @type {GraphHistory}
 */
let history = null;

/**
 * @type {StorageManager}
 */
let storage;


// Set up auto-save
let saveTimeout;
const autoSave = () => {
    if (saveTimeout) {
        clearTimeout(saveTimeout);
    }
    saveTimeout = setTimeout(() => saveGraphState(graph, history), 1000);
};

addEventListener("message", async (ev) => {

    switch (ev.data.event) {
        case "INIT": {
            if (!graph) {
                /**
                 * @type [Graph, History]
                 */
                [graph, history] = await initializeGraphWithPersistence(ev.data.name);
            }

            graph.setProperties({ name: ev.data.name })

            graph.subscribe((graph_ev) => {
                self.postMessage(graph_ev);
            });

            break;
        }
        case "ADD_NODE": {
            if (!graph) {
                console.error("Graph worker is not initialized");
                return;
            }

            if (!ev.data.node.process) {
                console.error("Node process is not defined");
                return;
            }
            /**
             * @type {import("./node.js").Node}
             */
            let node = ev.data.node;
            graph.addNode(node.id, node.process, node.metadata);
            history.processEvents(graph);
            autoSave();
            break;
        }
        case "UPDATE_NODE_METADATA": {
            if (!graph) {
                console.error("Graph worker is not initialized");
                return;
            }

            if (!ev.data.node.process) {
                console.error("Node process is not defined");
                return;
            }

           
            let node = ev.data.node;
            graph.setNodeMetadata(node.id, node.metadata);
            history.processEvents(graph);
            autoSave();
            break;
        }
        case "ADD_EDGE": {
            if (!graph) {
                console.error("Graph worker is not initialized");
                return;
            }

            if (!ev.data.edge) {
                return;
            }
            const from = ev.data.edge.from;
            const to = ev.data.edge.to;
            graph.addOutport(from.port.id, from.actor, from.port.name, true, from.port.metadata);
            history.processEvents(graph);
            autoSave();
            graph.addInport(to.port.id, to.actor, to.port.name, true, to.port.metadata);
            history.processEvents(graph);
            autoSave();
            console.log("addConnection", from.actor, from.port.id, to.actor, to.port.id, ev.data.edge.metadata)
            graph.addConnection(from.actor, from.port.id, to.actor, to.port.id, ev.data.edge.metadata);
            history.processEvents(graph);
            autoSave();
            break;
        }
        case "ADD_GROUP": {
            if (!graph) {
                console.error("Graph worker is not initialized");
                return;
            }

            if (!ev.data.group) {
                return;
            }
            
            graph.addGroup(ev.data.group.id, ev.data.group.nodes, ev.data.group.metadata);
            history.processEvents(graph);
            autoSave();
            break;
        }
        case "ADD_IIP": {
            if (!graph) {
                console.error("Graph worker is not initialized");
                return;
            }

            if (!ev.data.iip) {
                return;
            }

            graph.addInport(ev.data.iip.to.portId, ev.data.iip.to.nodeId, ev.data.iip.to.port, true, {
                trait: ev.data.iip.trait,
                position: ev.data.iip.position,
                data: ev.data.iip.data,
                port_name: ev.data.iip.to.port
            });
            history.processEvents(graph);
            graph.addInitial(ev.data.iip.data, ev.data.iip.to.nodeId, ev.data.iip.to.portId, {
                trait: ev.data.iip.trait,
                position: ev.data.iip.position,
                data: ev.data.iip.data,
                port_name: ev.data.iip.to.port
            });
            history.processEvents(graph);
            autoSave();
            break;
        }
        case "UPDATE_GROUP_METADATA": {
            if (typeof (ev.data.data) === "object") {
                const metadata = Object.assign({}, ev.data.data);
                delete metadata["id"];
                graph.setGroupMetadata(ev.data.data.id, metadata);
                history.processEvents(graph);
                autoSave();
            }
            break;
        }
    }


});

// fetch("../../crates/reflow_network/pkg/reflow_network_bg.wasm").then(async (res) => {
//     initSync(await res.arrayBuffer());
//     self.postMessage("READY");
// });



async function initializeGraphWithPersistence(name) {
    // Create graph and history
    let [graph, history] = Graph.withHistory();

    // Create storage manager
    storage = GraphHistory.createStorageManager(name, 'history');

    // Initialize the database
    await storage.initDatabase()

    // Load existing state if available
    try {
        // Try IndexedDB first
        const snapshot = await storage.loadFromIndexedDB('latest');
        if (snapshot) {
            history = GraphHistory.loadFromSnapshot(snapshot, graph);
        }
        // Fall back to LocalStorage
        // const localSnapshot = storage.loadFromLocalStorage('latest');
        else if (localSnapshot) {
            history = GraphHistory.loadFromSnapshot(localSnapshot, graph);
        }
    } catch (error) {
        console.warn('No previous state found:', error);
    }

    self.postMessage({ type: "GRAPH_LOADED", graph: graph.toJSON() });


    return [graph, history];
}

/**
 * 
 * @param {Graph} graph 
 * @param {GraphHistory} history 
 */
async function saveGraphState(graph, history) {

    try {
        // Try to save to IndexedDB first
        await storage.saveToIndexedDB('latest', graph, history);
    } catch (error) {
        console.warn('Failed to save to IndexedDB, falling back to LocalStorage:', error);

        // Fall back to LocalStorage
        try {
            storage.saveToLocalStorage('latest', graph, history);
        } catch (storageError) {
            console.error('Failed to save state:', storageError);
        }
    }
}