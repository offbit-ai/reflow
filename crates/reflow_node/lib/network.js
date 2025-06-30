const reflow = require('../index.node');


class Network {
    static ReflowNetwork = reflow.Network();
    constructor() {
        this.network = new Network.ReflowNetwork();
    }

    /**
     * Register a new actor in the network.
     * @param {string} name 
     * @param {Object} actor 
     */
    registerActor(name, actor) {
        Network.ReflowNetwork.prototype.registerActor(this.network, name, actor);
    }

    /**
     * Add a new actor to the network.
     * @param {string} name 
     * @param {string} process 
     * @param {Object | undefined} metadata 
     */
    addNode(name, process, metadata = {}) {
        Network.ReflowNetwork.prototype.addNode(this.network, name, process, metadata);
    }

    /**
     * Add connection between two nodes.
     * @param {import('./graph').GraphConnection} connection
     */
    addConnection(connection) {
        Network.ReflowNetwork.prototype.addConnection(this.network, connection);
    }

    /**
     * Add initial data to a node's port to trigger the graph.
     * @param {import('./graph').GraphIIP} iip 
     */
    addInitial(iip) {
        Network.ReflowNetwork.prototype.addInitial(this.network, iip);
    }

    start() {
        return Network.ReflowNetwork.prototype.start(this.network);
    }

    shutdown() {
        return Network.ReflowNetwork.prototype.shutdown(this.network);
    }  
    
    isRunning() {
        return Network.ReflowNetwork.prototype.isRunning(this.network);
    }
}


module.exports = {
    Network
}