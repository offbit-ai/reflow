# Building a Visual Graph Editor

Complete tutorial for creating a visual graph editor using Reflow's WebAssembly APIs.

## Overview

This tutorial walks through building a complete visual graph editor that allows users to:
- Create and edit graphs visually
- Add nodes by dragging from a component palette
- Connect nodes with visual links
- Configure node properties through forms
- Execute workflows and see real-time results
- Save and load graph files

## Prerequisites

- Basic HTML, CSS, and JavaScript knowledge
- Understanding of Reflow's graph concepts
- Node.js and npm installed

## Project Setup

### 1. Initialize Project

```bash
mkdir reflow-visual-editor
cd reflow-visual-editor
npm init -y
```

### 2. Install Dependencies

```bash
# Core dependencies
npm install reflow-network-wasm

# Development dependencies
npm install --save-dev webpack webpack-cli webpack-dev-server
npm install --save-dev html-webpack-plugin css-loader style-loader
npm install --save-dev @babel/core @babel/preset-env babel-loader
```

### 3. Project Structure

```
reflow-visual-editor/
├── src/
│   ├── index.html
│   ├── index.js
│   ├── style.css
│   ├── components/
│   │   ├── Graph.js
│   │   ├── Node.js
│   │   ├── Connection.js
│   │   ├── Palette.js
│   │   └── PropertyPanel.js
│   ├── utils/
│   │   ├── drag-drop.js
│   │   ├── events.js
│   │   └── serialization.js
│   └── workers/
│       └── execution-worker.js
├── webpack.config.js
└── package.json
```

## Core Implementation

### 1. Basic HTML Structure

```html
<!-- src/index.html -->
<!DOCTYPE html>
<html lang="en">
<head>
    <meta charset="UTF-8">
    <meta name="viewport" content="width=device-width, initial-scale=1.0">
    <title>Reflow Visual Editor</title>
</head>
<body>
    <div id="app">
        <header class="toolbar">
            <div class="toolbar-group">
                <button id="new-graph">New</button>
                <button id="open-graph">Open</button>
                <button id="save-graph">Save</button>
            </div>
            <div class="toolbar-group">
                <button id="run-graph">Run</button>
                <button id="stop-graph">Stop</button>
                <button id="validate-graph">Validate</button>
            </div>
            <div class="toolbar-group">
                <button id="auto-layout">Auto Layout</button>
                <button id="zoom-fit">Zoom to Fit</button>
            </div>
        </header>
        
        <div class="editor-container">
            <div class="sidebar">
                <div class="component-palette" id="palette">
                    <h3>Components</h3>
                    <div class="palette-category" data-category="data">
                        <h4>Data Operations</h4>
                        <div class="palette-items">
                            <!-- Component items will be populated by JavaScript -->
                        </div>
                    </div>
                    <div class="palette-category" data-category="flow">
                        <h4>Flow Control</h4>
                        <div class="palette-items">
                            <!-- Component items will be populated by JavaScript -->
                        </div>
                    </div>
                    <div class="palette-category" data-category="io">
                        <h4>Input/Output</h4>
                        <div class="palette-items">
                            <!-- Component items will be populated by JavaScript -->
                        </div>
                    </div>
                </div>
            </div>
            
            <div class="graph-canvas-container">
                <svg id="graph-canvas" class="graph-canvas">
                    <defs>
                        <marker id="arrowhead" markerWidth="10" markerHeight="7" 
                                refX="10" refY="3.5" orient="auto">
                            <polygon points="0 0, 10 3.5, 0 7" fill="#666" />
                        </marker>
                    </defs>
                    <g id="connections-layer"></g>
                    <g id="nodes-layer"></g>
                </svg>
                
                <div class="canvas-overlay">
                    <div class="zoom-controls">
                        <button id="zoom-in">+</button>
                        <button id="zoom-out">-</button>
                        <span id="zoom-level">100%</span>
                    </div>
                </div>
            </div>
            
            <div class="properties-panel" id="properties-panel">
                <h3>Properties</h3>
                <div id="property-form">
                    <p>Select a node to edit properties</p>
                </div>
            </div>
        </div>
        
        <div class="status-bar">
            <span id="status-text">Ready</span>
            <div class="status-indicators">
                <span id="node-count">0 nodes</span>
                <span id="connection-count">0 connections</span>
            </div>
        </div>
    </div>
</body>
</html>
```

### 2. Main Application Class

```javascript
// src/index.js
import { Graph } from 'reflow-network-wasm';
import GraphEditor from './components/Graph.js';
import ComponentPalette from './components/Palette.js';
import PropertyPanel from './components/PropertyPanel.js';
import './style.css';

class VisualEditor {
    constructor() {
        this.graph = new Graph("VisualWorkflow", true, {});
        this.graphEditor = new GraphEditor(this.graph, '#graph-canvas');
        this.palette = new ComponentPalette('#palette');
        this.propertyPanel = new PropertyPanel('#properties-panel');
        
        this.selectedNode = null;
        this.isExecuting = false;
        
        this.initializeEventListeners();
        this.initializeComponents();
    }
    
    initializeEventListeners() {
        // Toolbar events
        document.getElementById('new-graph').addEventListener('click', () => this.newGraph());
        document.getElementById('open-graph').addEventListener('click', () => this.openGraph());
        document.getElementById('save-graph').addEventListener('click', () => this.saveGraph());
        document.getElementById('run-graph').addEventListener('click', () => this.runGraph());
        document.getElementById('stop-graph').addEventListener('click', () => this.stopGraph());
        document.getElementById('validate-graph').addEventListener('click', () => this.validateGraph());
        document.getElementById('auto-layout').addEventListener('click', () => this.autoLayout());
        document.getElementById('zoom-fit').addEventListener('click', () => this.zoomToFit());
        
        // Zoom controls
        document.getElementById('zoom-in').addEventListener('click', () => this.graphEditor.zoomIn());
        document.getElementById('zoom-out').addEventListener('click', () => this.graphEditor.zoomOut());
        
        // Graph events
        this.graphEditor.on('nodeSelected', (node) => this.selectNode(node));
        this.graphEditor.on('nodeDeselected', () => this.deselectNode());
        this.graphEditor.on('nodeAdded', (node) => this.updateStatus());
        this.graphEditor.on('nodeRemoved', (node) => this.updateStatus());
        this.graphEditor.on('connectionAdded', (connection) => this.updateStatus());
        this.graphEditor.on('connectionRemoved', (connection) => this.updateStatus());
        
        // Palette events
        this.palette.on('componentDragStart', (component) => this.handleComponentDrag(component));
        
        // Property panel events
        this.propertyPanel.on('propertyChanged', (property, value) => this.updateNodeProperty(property, value));
    }
    
    initializeComponents() {
        this.palette.loadComponents([
            // Data Operations
            { 
                name: 'Map', 
                category: 'data', 
                component: 'MapActor',
                description: 'Transform data using a function',
                icon: '🔄',
                ports: {
                    input: [{ name: 'input', type: 'any' }],
                    output: [{ name: 'output', type: 'any' }]
                }
            },
            { 
                name: 'Filter', 
                category: 'data', 
                component: 'FilterActor',
                description: 'Filter data based on conditions',
                icon: '🔍',
                ports: {
                    input: [{ name: 'input', type: 'any' }],
                    output: [{ name: 'output', type: 'any' }]
                }
            },
            { 
                name: 'Aggregate', 
                category: 'data', 
                component: 'AggregateActor',
                description: 'Aggregate multiple inputs',
                icon: '📊',
                ports: {
                    input: [{ name: 'input', type: 'any' }],
                    output: [{ name: 'output', type: 'any' }]
                }
            },
            
            // Flow Control
            { 
                name: 'Conditional', 
                category: 'flow', 
                component: 'ConditionalActor',
                description: 'Branch based on condition',
                icon: '🔀',
                ports: {
                    input: [{ name: 'input', type: 'any' }],
                    output: [
                        { name: 'true', type: 'any' },
                        { name: 'false', type: 'any' }
                    ]
                }
            },
            { 
                name: 'Merge', 
                category: 'flow', 
                component: 'MergeActor',
                description: 'Merge multiple inputs',
                icon: '🔗',
                ports: {
                    input: [
                        { name: 'input1', type: 'any' },
                        { name: 'input2', type: 'any' }
                    ],
                    output: [{ name: 'output', type: 'any' }]
                }
            },
            
            // Input/Output
            { 
                name: 'HTTP Request', 
                category: 'io', 
                component: 'HttpRequestActor',
                description: 'Make HTTP requests',
                icon: '🌐',
                ports: {
                    input: [{ name: 'url', type: 'string' }],
                    output: [
                        { name: 'response', type: 'object' },
                        { name: 'error', type: 'object' }
                    ]
                }
            },
            { 
                name: 'Logger', 
                category: 'io', 
                component: 'LoggerActor',
                description: 'Log messages',
                icon: '📝',
                ports: {
                    input: [{ name: 'message', type: 'any' }],
                    output: []
                }
            }
        ]);
        
        this.updateStatus();
    }
    
    newGraph() {
        if (this.hasUnsavedChanges()) {
            if (!confirm('You have unsaved changes. Create a new graph anyway?')) {
                return;
            }
        }
        
        this.graph = new Graph("VisualWorkflow", true, {});
        this.graphEditor.setGraph(this.graph);
        this.deselectNode();
        this.updateStatus();
        this.setStatus('New graph created');
    }
    
    async openGraph() {
        const input = document.createElement('input');
        input.type = 'file';
        input.accept = '.json';
        input.onchange = async (e) => {
            const file = e.target.files[0];
            if (file) {
                try {
                    const text = await file.text();
                    const graphData = JSON.parse(text);
                    this.graph = Graph.fromJson(graphData);
                    this.graphEditor.setGraph(this.graph);
                    this.deselectNode();
                    this.updateStatus();
                    this.setStatus(`Opened: ${file.name}`);
                } catch (error) {
                    alert(`Error opening file: ${error.message}`);
                }
            }
        };
        input.click();
    }
    
    saveGraph() {
        try {
            const graphData = this.graph.toJson();
            const blob = new Blob([JSON.stringify(graphData, null, 2)], 
                                { type: 'application/json' });
            const url = URL.createObjectURL(blob);
            
            const a = document.createElement('a');
            a.href = url;
            a.download = `${this.graph.name || 'workflow'}.json`;
            a.click();
            
            URL.revokeObjectURL(url);
            this.setStatus('Graph saved');
        } catch (error) {
            alert(`Error saving graph: ${error.message}`);
        }
    }
    
    async runGraph() {
        try {
            this.setStatus('Validating graph...');
            const validation = this.graph.validate();
            
            if (!validation.isValid) {
                alert(`Graph validation failed:\n${validation.errors.join('\n')}`);
                return;
            }
            
            this.setStatus('Starting execution...');
            this.isExecuting = true;
            
            // Use Web Worker for graph execution
            if (!this.executionWorker) {
                this.executionWorker = new Worker('./workers/execution-worker.js');
                this.executionWorker.onmessage = (e) => this.handleExecutionMessage(e);
            }
            
            this.executionWorker.postMessage({
                type: 'execute',
                graph: this.graph.toJson()
            });
            
            this.updateToolbarState();
        } catch (error) {
            this.setStatus(`Execution error: ${error.message}`);
            this.isExecuting = false;
            this.updateToolbarState();
        }
    }
    
    stopGraph() {
        if (this.executionWorker) {
            this.executionWorker.postMessage({ type: 'stop' });
        }
        this.isExecuting = false;
        this.updateToolbarState();
        this.setStatus('Execution stopped');
    }
    
    validateGraph() {
        try {
            const validation = this.graph.validate();
            
            if (validation.isValid) {
                this.setStatus('Graph is valid');
                // Highlight valid state in UI
                this.graphEditor.highlightValidation(validation);
            } else {
                this.setStatus(`Validation failed: ${validation.errors.length} errors`);
                // Show validation errors in UI
                this.graphEditor.showValidationErrors(validation.errors);
                
                // Show detailed errors in console or modal
                console.log('Validation errors:', validation.errors);
            }
        } catch (error) {
            this.setStatus(`Validation error: ${error.message}`);
        }
    }
    
    autoLayout() {
        try {
            this.setStatus('Calculating layout...');
            const positions = this.graph.calculateLayout({
                algorithm: 'hierarchical',
                nodeSpacing: 120,
                layerSpacing: 80
            });
            
            this.graphEditor.animateToPositions(positions);
            this.setStatus('Layout applied');
        } catch (error) {
            this.setStatus(`Layout error: ${error.message}`);
        }
    }
    
    zoomToFit() {
        this.graphEditor.zoomToFit();
        this.setStatus('Zoomed to fit');
    }
    
    selectNode(node) {
        this.selectedNode = node;
        this.propertyPanel.showNodeProperties(node);
        this.graphEditor.highlightNode(node.id);
    }
    
    deselectNode() {
        this.selectedNode = null;
        this.propertyPanel.clear();
        this.graphEditor.clearHighlight();
    }
    
    updateNodeProperty(property, value) {
        if (this.selectedNode) {
            this.selectedNode.metadata[property] = value;
            this.graphEditor.updateNode(this.selectedNode);
            this.setStatus(`Updated ${property}`);
        }
    }
    
    handleComponentDrag(component) {
        this.graphEditor.enableDropZone(component);
    }
    
    handleExecutionMessage(e) {
        const { type, data } = e.data;
        
        switch (type) {
            case 'progress':
                this.setStatus(`Executing... ${data.progress}%`);
                this.graphEditor.updateExecutionProgress(data);
                break;
                
            case 'completed':
                this.setStatus('Execution completed successfully');
                this.isExecuting = false;
                this.updateToolbarState();
                this.graphEditor.showExecutionResults(data.results);
                break;
                
            case 'error':
                this.setStatus(`Execution failed: ${data.error}`);
                this.isExecuting = false;
                this.updateToolbarState();
                this.graphEditor.showExecutionError(data);
                break;
                
            case 'nodeExecuted':
                this.graphEditor.highlightExecutedNode(data.nodeId);
                break;
        }
    }
    
    updateStatus() {
        const nodeCount = this.graph.getNodes().length;
        const connectionCount = this.graph.getConnections().length;
        
        document.getElementById('node-count').textContent = `${nodeCount} nodes`;
        document.getElementById('connection-count').textContent = `${connectionCount} connections`;
    }
    
    updateToolbarState() {
        document.getElementById('run-graph').disabled = this.isExecuting;
        document.getElementById('stop-graph').disabled = !this.isExecuting;
    }
    
    setStatus(message) {
        document.getElementById('status-text').textContent = message;
        console.log(`Status: ${message}`);
    }
    
    hasUnsavedChanges() {
        // Implement change tracking logic
        return false;
    }
}

// Initialize application when DOM is ready
document.addEventListener('DOMContentLoaded', () => {
    new VisualEditor();
});
```

### 3. Graph Editor Component

```javascript
// src/components/Graph.js
import { EventEmitter } from '../utils/events.js';
import Node from './Node.js';
import Connection from './Connection.js';

class GraphEditor extends EventEmitter {
    constructor(graph, canvasSelector) {
        super();
        this.graph = graph;
        this.canvas = document.querySelector(canvasSelector);
        this.nodesLayer = this.canvas.querySelector('#nodes-layer');
        this.connectionsLayer = this.canvas.querySelector('#connections-layer');
        
        this.nodes = new Map();
        this.connections = new Map();
        this.scale = 1;
        this.panX = 0;
        this.panY = 0;
        
        this.dragState = null;
        this.connectionDragState = null;
        
        this.initializeEventListeners();
        this.updateView();
    }
    
    initializeEventListeners() {
        // Mouse events for panning and selection
        this.canvas.addEventListener('mousedown', (e) => this.handleMouseDown(e));
        this.canvas.addEventListener('mousemove', (e) => this.handleMouseMove(e));
        this.canvas.addEventListener('mouseup', (e) => this.handleMouseUp(e));
        this.canvas.addEventListener('wheel', (e) => this.handleWheel(e));
        
        // Drag and drop for components
        this.canvas.addEventListener('dragover', (e) => e.preventDefault());
        this.canvas.addEventListener('drop', (e) => this.handleDrop(e));
        
        // Keyboard events
        document.addEventListener('keydown', (e) => this.handleKeyDown(e));
    }
    
    setGraph(graph) {
        this.graph = graph;
        this.updateView();
    }
    
    updateView() {
        this.clearView();
        this.renderConnections();
        this.renderNodes();
    }
    
    clearView() {
        this.nodesLayer.innerHTML = '';
        this.connectionsLayer.innerHTML = '';
        this.nodes.clear();
        this.connections.clear();
    }
    
    renderNodes() {
        const graphNodes = this.graph.getNodes();
        
        graphNodes.forEach(nodeData => {
            const node = new Node(nodeData, this);
            this.nodes.set(nodeData.id, node);
            this.nodesLayer.appendChild(node.element);
        });
    }
    
    renderConnections() {
        const graphConnections = this.graph.getConnections();
        
        graphConnections.forEach(connectionData => {
            const connection = new Connection(connectionData, this);
            this.connections.set(connection.id, connection);
            this.connectionsLayer.appendChild(connection.element);
        });
    }
    
    addNode(componentType, position) {
        const nodeId = `node_${Date.now()}`;
        const nodeData = {
            id: nodeId,
            component: componentType.component,
            metadata: {
                x: position.x,
                y: position.y,
                label: componentType.name,
                ...componentType.defaultProperties
            }
        };
        
        this.graph.addNode(nodeId, componentType.component, nodeData.metadata);
        
        const node = new Node(nodeData, this);
        this.nodes.set(nodeId, node);
        this.nodesLayer.appendChild(node.element);
        
        this.emit('nodeAdded', nodeData);
        return node;
    }
    
    removeNode(nodeId) {
        const node = this.nodes.get(nodeId);
        if (node) {
            // Remove all connections to this node
            const connections = this.graph.getConnections()
                .filter(conn => conn.fromNode === nodeId || conn.toNode === nodeId);
            
            connections.forEach(conn => this.removeConnection(conn.id));
            
            // Remove node from graph
            this.graph.removeNode(nodeId);
            
            // Remove from UI
            node.element.remove();
            this.nodes.delete(nodeId);
            
            this.emit('nodeRemoved', { id: nodeId });
        }
    }
    
    addConnection(fromNode, fromPort, toNode, toPort) {
        try {
            const connectionId = this.graph.addConnection(fromNode, fromPort, toNode, toPort, {});
            
            const connectionData = {
                id: connectionId,
                fromNode,
                fromPort,
                toNode,
                toPort
            };
            
            const connection = new Connection(connectionData, this);
            this.connections.set(connectionId, connection);
            this.connectionsLayer.appendChild(connection.element);
            
            this.emit('connectionAdded', connectionData);
            return connection;
        } catch (error) {
            console.error('Failed to create connection:', error);
            throw error;
        }
    }
    
    removeConnection(connectionId) {
        const connection = this.connections.get(connectionId);
        if (connection) {
            this.graph.removeConnection(connectionId);
            connection.element.remove();
            this.connections.delete(connectionId);
            
            this.emit('connectionRemoved', { id: connectionId });
        }
    }
    
    getNodePosition(nodeId) {
        const node = this.nodes.get(nodeId);
        return node ? node.getPosition() : null;
    }
    
    updateNodePosition(nodeId, position) {
        const node = this.nodes.get(nodeId);
        if (node) {
            node.setPosition(position);
            this.updateConnectionsForNode(nodeId);
        }
    }
    
    updateConnectionsForNode(nodeId) {
        this.connections.forEach(connection => {
            if (connection.fromNode === nodeId || connection.toNode === nodeId) {
                connection.updatePath();
            }
        });
    }
    
    handleMouseDown(e) {
        if (e.target === this.canvas) {
            this.startPanning(e);
        }
    }
    
    handleMouseMove(e) {
        if (this.dragState?.type === 'pan') {
            this.updatePanning(e);
        } else if (this.connectionDragState) {
            this.updateConnectionDrag(e);
        }
    }
    
    handleMouseUp(e) {
        if (this.dragState?.type === 'pan') {
            this.endPanning();
        } else if (this.connectionDragState) {
            this.endConnectionDrag(e);
        }
    }
    
    handleWheel(e) {
        e.preventDefault();
        const delta = e.deltaY > 0 ? 0.9 : 1.1;
        this.zoom(delta, { x: e.clientX, y: e.clientY });
    }
    
    handleDrop(e) {
        e.preventDefault();
        const componentData = JSON.parse(e.dataTransfer.getData('component'));
        const rect = this.canvas.getBoundingClientRect();
        const position = this.screenToWorld({
            x: e.clientX - rect.left,
            y: e.clientY - rect.top
        });
        
        this.addNode(componentData, position);
    }
    
    handleKeyDown(e) {
        if (e.key === 'Delete' && this.selectedNode) {
            this.removeNode(this.selectedNode.id);
        }
    }
    
    startPanning(e) {
        this.dragState = {
            type: 'pan',
            startX: e.clientX,
            startY: e.clientY,
            initialPanX: this.panX,
            initialPanY: this.panY
        };
    }
    
    updatePanning(e) {
        if (this.dragState?.type === 'pan') {
            const dx = e.clientX - this.dragState.startX;
            const dy = e.clientY - this.dragState.startY;
            
            this.panX = this.dragState.initialPanX + dx;
            this.panY = this.dragState.initialPanY + dy;
            
            this.updateTransform();
        }
    }
    
    endPanning() {
        this.dragState = null;
    }
    
    startConnectionDrag(fromNode, fromPort, startPosition) {
        this.connectionDragState = {
            fromNode,
            fromPort,
            startPosition,
            currentPosition: startPosition
        };
        
        // Create temporary connection line
        this.createTempConnectionLine();
    }
    
    updateConnectionDrag(e) {
        if (this.connectionDragState) {
            const rect = this.canvas.getBoundingClientRect();
            this.connectionDragState.currentPosition = {
                x: e.clientX - rect.left,
                y: e.clientY - rect.top
            };
            
            this.updateTempConnectionLine();
        }
    }
    
    endConnectionDrag(e) {
        if (this.connectionDragState) {
            // Find target node and port
            const target = this.findConnectionTarget(e);
            
            if (target) {
                try {
                    this.addConnection(
                        this.connectionDragState.fromNode,
                        this.connectionDragState.fromPort,
                        target.nodeId,
                        target.portName
                    );
                } catch (error) {
                    console.error('Connection failed:', error);
                }
            }
            
            this.removeTempConnectionLine();
            this.connectionDragState = null;
        }
    }
    
    zoom(factor, center) {
        const newScale = Math.max(0.1, Math.min(3, this.scale * factor));
        
        if (center) {
            const worldCenter = this.screenToWorld(center);
            this.scale = newScale;
            const newScreenCenter = this.worldToScreen(worldCenter);
            
            this.panX += center.x - newScreenCenter.x;
            this.panY += center.y - newScreenCenter.y;
        } else {
            this.scale = newScale;
        }
        
        this.updateTransform();
        this.updateZoomDisplay();
    }
    
    zoomIn() {
        this.zoom(1.2);
    }
    
    zoomOut() {
        this.zoom(0.8);
    }
    
    zoomToFit() {
        if (this.nodes.size === 0) return;
        
        // Calculate bounding box of all nodes
        let minX = Infinity, minY = Infinity;
        let maxX = -Infinity, maxY = -Infinity;
        
        this.nodes.forEach(node => {
            const pos = node.getPosition();
            minX = Math.min(minX, pos.x);
            minY = Math.min(minY, pos.y);
            maxX = Math.max(maxX, pos.x + 120); // Node width
            maxY = Math.max(maxY, pos.y + 80);  // Node height
        });
        
        const padding = 50;
        const contentWidth = maxX - minX + 2 * padding;
        const contentHeight = maxY - minY + 2 * padding;
        
        const canvasRect = this.canvas.getBoundingClientRect();
        const scaleX = canvasRect.width / contentWidth;
        const scaleY = canvasRect.height / contentHeight;
        
        this.scale = Math.min(scaleX, scaleY, 1);
        this.panX = (canvasRect.width - contentWidth * this.scale) / 2 - (minX - padding) * this.scale;
        this.panY = (canvasRect.height - contentHeight * this.scale) / 2 - (minY - padding) * this.scale;
        
        this.updateTransform();
        this.updateZoomDisplay();
    }
    
    updateTransform() {
        const transform = `translate(${this.panX}px, ${this.panY}px) scale(${this.scale})`;
        this.nodesLayer.style.transform = transform;
        this.connectionsLayer.style.transform = transform;
    }
    
    updateZoomDisplay() {
        const zoomPercent = Math.round(this.scale * 100);
        document.getElementById('zoom-level').textContent = `${zoomPercent}%`;
    }
    
    screenToWorld(screenPos) {
        return {
            x: (screenPos.x - this.panX) / this.scale,
            y: (screenPos.y - this.panY) / this.scale
        };
    }
    
    worldToScreen(worldPos) {
        return {
            x: worldPos.x * this.scale + this.panX,
            y: worldPos.y * this.scale + this.panY
        };
    }
    
    // Additional methods for animations, validation display, etc.
    animateToPositions
