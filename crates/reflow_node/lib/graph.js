const reflow = require('../index.node');



/**
 * Graph structure for managing nodes and connections.
 */
class Graph {
    static ReflowGraph;
    constructor(name = "DefaultGraph") {
        Graph.ReflowGraph = reflow.Graph(name);
        this.inner = new Graph.ReflowGraph();
    }

    /**
     * Add a new node to the graph.
     * @param {string} name 
     * @param {string} process 
     * @param {Object} metadata 
     */
    addNode(name, process, metadata = {}) {
        Graph.ReflowGraph.prototype.addNode(this.inner, name, process, metadata);
    }

    /**
     * Remove a node from the graph.
     * @param {string} name 
     */
    removeNode(name) {
        Graph.ReflowGraph.prototype.removeNode(this.inner, name);
    }

    /**
     * Add a connection between two nodes.
     * @param {string} node1 
     * @param {string} port1 
     * @param {string} node2 
     * @param {string} port2 
     */
    addConnection(node1, port1, node2, port2) {
        Graph.ReflowGraph.prototype.addConnection(this.inner, node1, port1, node2, port2);
    }
    /**
     * Remove a connection between two nodes.
     * @param {string} node1 
     * @param {string} port1 
     * @param {string} node2 
     * @param {string} port2 
     */
    removeConnection(node1, port1, node2, port2) {
        Graph.ReflowGraph.prototype.removeConnection(this.inner, node1, port1, node2, port2);
    }

    /**
     * Add initial data to a node's port to trigger the graph.
     * @param {Object} data 
     * @param {string} node 
     * @param {string} port 
     */
    addInitial(data, node, port) {
        Graph.ReflowGraph.prototype.addInitial(this.inner, data, node, port);
    }

    /**
     * Remove initial data from a node's port.
     * This is useful for clearing initial data that was set to trigger the graph.
     * @param {string} node 
     * @param {string} port 
     */
    removeInitial(node, port) {
        Graph.ReflowGraph.prototype.removeInitial(this.inner, node, port);
    }

    /**
     * Add an inport to a node.
     * @param {string} portId 
     * @param {string} nodeId 
     * @param {string} portName 
     * @param {Object} metadata 
     */
    addInport(portId, nodeId, portName, metadata = {}) {
        Graph.ReflowGraph.prototype.addInport(this.inner, portId, nodeId, portName, metadata);
    }

    /**
     * Add an outport to a node.
     * @param {string} portId 
     * @param {string} nodeId 
     * @param {string} portName 
     * @param {Object} metadata 
     */
    addOutport(portId, nodeId, portName, metadata = {}) {
        Graph.ReflowGraph.prototype.addOutport(this.inner, portId, nodeId, portName, metadata);
    }

    /**
     * Remove an inport from a node.
     * @param {string} portId 
     */
    removeInport(portId) {
        Graph.ReflowGraph.prototype.removeInport(this.inner, portId);
    }

    /**
     * Remove an outport from a node.
     * @param {string} portId 
     */
    removeOutport(portId) {
        Graph.ReflowGraph.prototype.removeOutport(this.inner, portId);
    }

    /**
     * Add a group to the graph.
     * A group is a collection of nodes that can be managed together.
     * @param {string} name 
     * @param {string[]} nodes 
     * @param {Object} metadata 
     */
    addGroup(name, nodes = [], metadata = {}) {
        Graph.ReflowGraph.prototype.addGroup(this.inner, name, nodes, metadata);
    }

    /**
     * Remove a group from the graph.
     * @param {string} name 
     */
    removeGroup(name) {
        Graph.ReflowGraph.prototype.removeGroup(this.inner, name);
    }

    /**
     * Get all groups in the graph.
     * @returns {import('./graph').GraphIIP[]} Array of group objects.
     */
    getInitializers() {
        return Graph.ReflowGraph.prototype.getInitializers(this.inner);
    }

    /**
     * Get all connections in the graph.
     * @returns {import('./graph').GraphConnection[]} Array of connection objects.
     */
    getConnections() {
        return Graph.ReflowGraph.prototype.getConnections(this.inner);
    }

    /**
     * Get all nodes in the graph.
     * @returns {import('./graph').GraphNode[]} Array of node objects.
     */
    getNodes() {
        return Graph.ReflowGraph.prototype.getNodes(this.inner);
    }

    export() {
        return Graph.ReflowGraph.prototype.export(this.inner);
    }
}


module.exports = {
    Graph
}