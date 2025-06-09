# Building a ReactFlow Editor with Reflow Engine in a Web Worker

This tutorial demonstrates how to build a modern visual workflow editor using **ReactFlow** for the user interface and **Reflow engine** running in a Web Worker for graph execution and state management.

## Table of Contents

1. [Architecture Overview](#architecture-overview)
2. [Project Setup](#project-setup)
3. [Worker Integration](#worker-integration)
4. [ReactFlow Integration](#reactflow-integration)
5. [Custom Node Components](#custom-node-components)
6. [Real-time Communication](#real-time-communication)
7. [Advanced Features](#advanced-features)
8. [Complete Example](#complete-example)
9. [Performance Optimization](#performance-optimization)

## Architecture Overview

Our architecture separates concerns between the UI layer (ReactFlow) and the execution engine (Reflow WebAssembly):

```
┌─────────────────────────────────────────────────────────────────┐
│                    React Application                            │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐ │
│  │   ReactFlow     │  │  Component      │  │   Execution     │ │
│  │     Editor      │  │    Palette      │  │    Controls     │ │
│  └─────────────────┘  └─────────────────┘  └─────────────────┘ │
│            │                    │                    │         │
│            └────────────────────┼────────────────────┘         │
│                                 │                              │
└─────────────────────────────────┼──────────────────────────────┘
                                  │ PostMessage API
┌─────────────────────────────────┼──────────────────────────────┐
│                          Web Worker                             │
│  ┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐ │
│  │ Reflow WebAssm  │  │  Graph State    │  │   Persistence   │ │
│  │     Engine      │  │   Management    │  │   (IndexedDB)   │ │
│  └─────────────────┘  └─────────────────┘  └─────────────────┘ │
└─────────────────────────────────────────────────────────────────┘
```

**Benefits:**
- **Performance**: Heavy graph operations don't block the UI thread
- **Scalability**: Can handle large, complex workflows
- **Persistence**: Graph state maintained separately from UI state
- **Modularity**: Clear separation between presentation and logic

## Project Setup

### Prerequisites

- Node.js 18+ and npm/yarn
- Rust toolchain with `wasm-pack` installed
- Basic knowledge of React and TypeScript

### 1. Initialize React Project

```bash
# Create new React TypeScript project
npm create react-app@latest reflow-editor --template typescript
cd reflow-editor

# Install ReactFlow and dependencies
npm install reactflow
npm install @types/web
```

### 2. Install Reflow WebAssembly Package

```bash
# Build the Reflow WebAssembly package (from Reflow repo root)
cd crates/reflow_network
wasm-pack build --target web --out-dir pkg

# Copy the generated package to your React project
cp -r pkg/ /path/to/reflow-editor/src/reflow-wasm/
```

### 3. Project Structure

```
reflow-editor/
├── src/
│   ├── components/
│   │   ├── Editor/
│   │   │   ├── ReactFlowEditor.tsx
│   │   │   ├── CustomNodes/
│   │   │   └── CustomEdges/
│   │   ├── Palette/
│   │   │   └── ComponentPalette.tsx
│   │   └── Controls/
│   │       └── ExecutionControls.tsx
│   ├── workers/
│   │   └── reflow-worker.ts
│   ├── hooks/
│   │   ├── useReflowWorker.ts
│   │   └── useGraphSync.ts
│   ├── types/
│   │   └── reflow.ts
│   ├── reflow-wasm/          # Copied from Reflow build
│   └── App.tsx
```

## Worker Integration

### 1. Create the Reflow Worker

First, let's create the Web Worker that manages the Reflow engine:

```typescript
// src/workers/reflow-worker.ts
import { Graph, GraphHistory, StorageManager, initSync } from '../reflow-wasm/reflow_network.js';

// Worker state
let graph: Graph | null = null;
let history: GraphHistory | null = null;
let storage: StorageManager | null = null;

// Message types for type safety
export interface WorkerMessage {
  type: 'INIT' | 'ADD_NODE' | 'ADD_EDGE' | 'UPDATE_NODE' | 'ADD_GROUP' | 'EXECUTE';
  payload?: any;
}

export interface WorkerResponse {
  type: 'READY' | 'GRAPH_LOADED' | 'NODE_ADDED' | 'EDGE_ADDED' | 'ERROR';
  payload?: any;
}

// Initialize WebAssembly
fetch('/reflow_network_bg.wasm').then(async (res) => {
  initSync(await res.arrayBuffer());
  self.postMessage({ type: 'READY' } as WorkerResponse);
});

// Auto-save functionality
let saveTimeout: NodeJS.Timeout;
const autoSave = () => {
  if (saveTimeout) clearTimeout(saveTimeout);
  saveTimeout = setTimeout(() => saveGraphState(), 1000);
};

// Message handler
self.addEventListener('message', async (event: MessageEvent<WorkerMessage>) => {
  const { type, payload } = event.data;

  try {
    switch (type) {
      case 'INIT':
        await initializeGraph(payload.name);
        break;

      case 'ADD_NODE':
        if (!graph) throw new Error('Graph not initialized');
        addNode(payload);
        break;

      case 'ADD_EDGE':
        if (!graph) throw new Error('Graph not initialized');
        addEdge(payload);
        break;

      case 'UPDATE_NODE':
        if (!graph) throw new Error('Graph not initialized');
        updateNode(payload);
        break;

      case 'ADD_GROUP':
        if (!graph) throw new Error('Graph not initialized');
        addGroup(payload);
        break;

      case 'EXECUTE':
        if (!graph) throw new Error('Graph not initialized');
        executeGraph();
        break;

      default:
        console.warn('Unknown message type:', type);
    }
  } catch (error) {
    self.postMessage({
      type: 'ERROR',
      payload: { message: error.message }
    } as WorkerResponse);
  }
});

// Initialize graph with persistence
async function initializeGraph(name: string) {
  [graph, history] = Graph.withHistory();
  storage = GraphHistory.createStorageManager(name, 'history');
  
  await storage.initDatabase();

  // Load existing state
  try {
    const snapshot = await storage.loadFromIndexedDB('latest');
    if (snapshot) {
      history = GraphHistory.loadFromSnapshot(snapshot, graph);
    }
  } catch (error) {
    console.warn('No previous state found:', error);
  }

  // Subscribe to graph events
  graph.subscribe((graphEvent) => {
    self.postMessage({
      type: 'GRAPH_EVENT',
      payload: graphEvent
    } as WorkerResponse);
  });

  self.postMessage({
    type: 'GRAPH_LOADED',
    payload: { graph: graph.toJSON() }
  } as WorkerResponse);
}

// Graph operation functions
function addNode(nodeData: any) {
  if (!graph || !history) return;

  graph.addNode(nodeData.id, nodeData.process, nodeData.metadata);
  history.processEvents(graph);
  autoSave();

  self.postMessage({
    type: 'NODE_ADDED',
    payload: nodeData
  } as WorkerResponse);
}

function addEdge(edgeData: any) {
  if (!graph || !history) return;

  const { from, to } = edgeData;
  
  // Add ports if they don't exist
  graph.addOutport(from.port.id, from.actor, from.port.name, true, from.port.metadata);
  graph.addInport(to.port.id, to.actor, to.port.name, true, to.port.metadata);
  
  // Add connection
  graph.addConnection(from.actor, from.port.id, to.actor, to.port.id, edgeData.metadata);
  
  history.processEvents(graph);
  autoSave();

  self.postMessage({
    type: 'EDGE_ADDED',
    payload: edgeData
  } as WorkerResponse);
}

function updateNode(nodeData: any) {
  if (!graph || !history) return;

  graph.setNodeMetadata(nodeData.id, nodeData.metadata);
  history.processEvents(graph);
  autoSave();
}

function addGroup(groupData: any) {
  if (!graph || !history) return;

  graph.addGroup(groupData.id, groupData.nodes, groupData.metadata);
  history.processEvents(graph);
  autoSave();
}

function executeGraph() {
  if (!graph) return;
  
  // Implement graph execution logic here
  console.log('Executing graph:', graph.toJSON());
}

// Save graph state
async function saveGraphState() {
  if (!graph || !history || !storage) return;

  try {
    await storage.saveToIndexedDB('latest', graph, history);
  } catch (error) {
    console.warn('Failed to save to IndexedDB:', error);
    try {
      storage.saveToLocalStorage('latest', graph, history);
    } catch (storageError) {
      console.error('Failed to save state:', storageError);
    }
  }
}
```

### 2. Create Worker Hook

Create a React hook to manage the worker communication:

```typescript
// src/hooks/useReflowWorker.ts
import { useEffect, useRef, useCallback, useState } from 'react';
import type { WorkerMessage, WorkerResponse } from '../workers/reflow-worker';

export interface ReflowWorkerHook {
  isReady: boolean;
  sendMessage: (message: WorkerMessage) => void;
  addEventListener: (listener: (event: WorkerResponse) => void) => void;
  removeEventListener: (listener: (event: WorkerResponse) => void) => void;
}

export function useReflowWorker(): ReflowWorkerHook {
  const workerRef = useRef<Worker | null>(null);
  const [isReady, setIsReady] = useState(false);
  const listenersRef = useRef<Set<(event: WorkerResponse) => void>>(new Set());

  useEffect(() => {
    // Create worker
    workerRef.current = new Worker('/src/workers/reflow-worker.ts', {
      type: 'module'
    });

    // Handle worker messages
    const handleMessage = (event: MessageEvent<WorkerResponse>) => {
      const message = event.data;
      
      if (message.type === 'READY') {
        setIsReady(true);
      }

      // Notify all listeners
      listenersRef.current.forEach(listener => listener(message));
    };

    workerRef.current.addEventListener('message', handleMessage);

    return () => {
      workerRef.current?.terminate();
    };
  }, []);

  const sendMessage = useCallback((message: WorkerMessage) => {
    if (workerRef.current && isReady) {
      workerRef.current.postMessage(message);
    }
  }, [isReady]);

  const addEventListener = useCallback((listener: (event: WorkerResponse) => void) => {
    listenersRef.current.add(listener);
  }, []);

  const removeEventListener = useCallback((listener: (event: WorkerResponse) => void) => {
    listenersRef.current.delete(listener);
  }, []);

  return {
    isReady,
    sendMessage,
    addEventListener,
    removeEventListener
  };
}
```

## ReactFlow Integration

### 1. Main Editor Component

```typescript
// src/components/Editor/ReactFlowEditor.tsx
import React, { useCallback, useEffect, useState } from 'react';
import ReactFlow, {
  Node,
  Edge,
  addEdge,
  useNodesState,
  useEdgesState,
  Connection,
  ReactFlowProvider,
  Controls,
  Background,
  Panel,
} from 'reactflow';

import 'reactflow/dist/style.css';

import { useReflowWorker } from '../../hooks/useReflowWorker';
import { useGraphSync } from '../../hooks/useGraphSync';
import { ReflowNode } from './CustomNodes/ReflowNode';
import { ComponentPalette } from '../Palette/ComponentPalette';
import { ExecutionControls } from '../Controls/ExecutionControls';

// Custom node types
const nodeTypes = {
  reflow: ReflowNode,
};

export function ReactFlowEditor() {
  const [nodes, setNodes, onNodesChange] = useNodesState([]);
  const [edges, setEdges, onEdgesChange] = useEdgesState([]);
  const worker = useReflowWorker();

  // Sync ReactFlow state with Reflow worker
  const { syncToWorker, syncFromWorker } = useGraphSync(worker, setNodes, setEdges);

  useEffect(() => {
    if (worker.isReady) {
      // Initialize the graph in the worker
      worker.sendMessage({
        type: 'INIT',
        payload: { name: 'ReactFlow Graph' }
      });
    }
  }, [worker.isReady]);

  const onConnect = useCallback(
    (params: Edge | Connection) => {
      // Update ReactFlow state
      setEdges((eds) => addEdge(params, eds));
      
      // Sync to worker
      syncToWorker.addEdge({
        from: {
          actor: params.source,
          port: {
            id: `${params.source}-${params.sourceHandle}`,
            name: params.sourceHandle || 'output',
          }
        },
        to: {
          actor: params.target,
          port: {
            id: `${params.target}-${params.targetHandle}`,
            name: params.targetHandle || 'input',
          }
        }
      });
    },
    [setEdges, syncToWorker]
  );

  const onDrop = useCallback(
    (event: React.DragEvent) => {
      event.preventDefault();

      const reactFlowBounds = event.currentTarget.getBoundingClientRect();
      const type = event.dataTransfer.getData('application/reactflow');
      const position = {
        x: event.clientX - reactFlowBounds.left,
        y: event.clientY - reactFlowBounds.top,
      };

      const newNode: Node = {
        id: `${type}-${Date.now()}`,
        type: 'reflow',
        position,
        data: {
          label: type,
          process: type,
          inports: getDefaultInports(type),
          outports: getDefaultOutports(type),
        },
      };

      // Update ReactFlow state
      setNodes((nds) => nds.concat(newNode));
      
      // Sync to worker
      syncToWorker.addNode({
        id: newNode.id,
        process: type,
        metadata: {
          position,
          name: type,
          inports: newNode.data.inports,
          outports: newNode.data.outports,
        }
      });
    },
    [setNodes, syncToWorker]
  );

  const onDragOver = useCallback((event: React.DragEvent) => {
    event.preventDefault();
    event.dataTransfer.dropEffect = 'move';
  }, []);

  return (
    <div style={{ width: '100vw', height: '100vh', display: 'flex' }}>
      {/* Component Palette */}
      <ComponentPalette />
      
      {/* Main ReactFlow Editor */}
      <div style={{ flex: 1 }} onDrop={onDrop} onDragOver={onDragOver}>
        <ReactFlow
          nodes={nodes}
          edges={edges}
          onNodesChange={onNodesChange}
          onEdgesChange={onEdgesChange}
          onConnect={onConnect}
          nodeTypes={nodeTypes}
          fitView
        >
          <Controls />
          <Background />
          
          {/* Execution Controls Panel */}
          <Panel position="top-right">
            <ExecutionControls 
              onExecute={() => worker.sendMessage({ type: 'EXECUTE' })}
              isReady={worker.isReady}
            />
          </Panel>
        </ReactFlow>
      </div>
    </div>
  );
}

// Helper functions for default port configurations
function getDefaultInports(nodeType: string) {
  const configs = {
    'DataSource': [],
    'MapActor': [{ id: 'input', name: 'input', trait: 'data' }],
    'Logger': [{ id: 'input', name: 'input', trait: 'data' }],
    'FilterActor': [{ id: 'input', name: 'input', trait: 'data' }],
  };
  return configs[nodeType] || [{ id: 'input', name: 'input', trait: 'data' }];
}

function getDefaultOutports(nodeType: string) {
  const configs = {
    'DataSource': [{ id: 'output', name: 'output', trait: 'data' }],
    'MapActor': [{ id: 'output', name: 'output', trait: 'data' }],
    'Logger': [],
    'FilterActor': [{ id: 'output', name: 'output', trait: 'data' }],
  };
  return configs[nodeType] || [{ id: 'output', name: 'output', trait: 'data' }];
}
```

## Custom Node Components

### 1. Reflow Node Component

```typescript
// src/components/Editor/CustomNodes/ReflowNode.tsx
import React, { memo } from 'react';
import { Handle, Position } from 'reactflow';

interface ReflowNodeData {
  label: string;
  process: string;
  inports: Array<{ id: string; name: string; trait: string }>;
  outports: Array<{ id: string; name: string; trait: string }>;
}

interface ReflowNodeProps {
  data: ReflowNodeData;
  isConnectable: boolean;
}

export const ReflowNode = memo(({ data, isConnectable }: ReflowNodeProps) => {
  return (
    <div className="reflow-node">
      {/* Input Handles */}
      {data.inports.map((port, index) => (
        <Handle
          key={port.id}
          type="target"
          position={Position.Left}
          id={port.id}
          isConnectable={isConnectable}
          style={{
            top: `${20 + (index * 25)}px`,
            background: getPortColor(port.trait),
          }}
        />
      ))}

      {/* Node Content */}
      <div className="node-content">
        <div className="node-header">
          <strong>{data.label}</strong>
        </div>
        <div className="node-type">
          {data.process}
        </div>
        
        {/* Port Labels */}
        <div className="port-labels">
          <div className="input-labels">
            {data.inports.map((port) => (
              <div key={port.id} className="port-label">
                {port.name}
              </div>
            ))}
          </div>
          <div className="output-labels">
            {data.outports.map((port) => (
              <div key={port.id} className="port-label">
                {port.name}
              </div>
            ))}
          </div>
        </div>
      </div>

      {/* Output Handles */}
      {data.outports.map((port, index) => (
        <Handle
          key={port.id}
          type="source"
          position={Position.Right}
          id={port.id}
          isConnectable={isConnectable}
          style={{
            top: `${20 + (index * 25)}px`,
            background: getPortColor(port.trait),
          }}
        />
      ))}
    </div>
  );
});

// Utility function for port colors
function getPortColor(trait: string): string {
  const colors = {
    data: '#3b82f6',      // Blue for data
    control: '#ef4444',   // Red for control
    event: '#10b981',     // Green for events
    config: '#f59e0b',    // Yellow for configuration
  };
  return colors[trait] || '#6b7280'; // Gray as default
}
```

### 2. Node Styles

```css
/* src/components/Editor/CustomNodes/ReflowNode.css */
.reflow-node {
  background: #ffffff;
  border: 2px solid #e5e7eb;
  border-radius: 8px;
  padding: 10px;
  min-width: 150px;
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.1);
  position: relative;
}

.reflow-node:hover {
  border-color: #3b82f6;
}

.node-content {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.node-header {
  font-size: 14px;
  font-weight: bold;
  color: #1f2937;
}

.node-type {
  font-size: 12px;
  color: #6b7280;
  font-style: italic;
}

.port-labels {
  display: flex;
  justify-content: space-between;
  font-size: 10px;
  color: #9ca3af;
}

.input-labels,
.output-labels {
  display: flex;
  flex-direction: column;
  gap: 4px;
}

.port-label {
  line-height: 1.2;
}

/* ReactFlow handle overrides */
.react-flow__handle {
  width: 8px;
  height: 8px;
  border: 2px solid #ffffff;
}

.react-flow__handle-left {
  left: -6px;
}

.react-flow__handle-right {
  right: -6px;
}
```

## Real-time Communication

### 1. Graph Synchronization Hook

```typescript
// src/hooks/useGraphSync.ts
import { useCallback, useEffect } from 'react';
import { Node, Edge } from 'reactflow';
import type { ReflowWorkerHook } from './useReflowWorker';

export function useGraphSync(
  worker: ReflowWorkerHook,
  setNodes: React.Dispatch<React.SetStateAction<Node[]>>,
  setEdges: React.Dispatch<React.SetStateAction<Edge[]>>
) {
  
  // Listen to worker events and sync to ReactFlow
  useEffect(() => {
    const handleWorkerMessage = (message: any) => {
      switch (message.type) {
        case 'GRAPH_LOADED':
          syncGraphFromWorker(message.payload.graph);
          break;
          
        case 'NODE_ADDED':
          // Handle real-time node additions from other sources
          break;
          
        case 'EDGE_ADDED':
          // Handle real-time edge additions from other sources
          break;
          
        case 'GRAPH_EVENT':
          // Handle live graph execution events
          console.log('Graph event:', message.payload);
          break;
      }
    };

    worker.addEventListener(handleWorkerMessage);
    
    return () => {
      worker.removeEventListener(handleWorkerMessage);
    };
  }, [worker]);

  // Convert Reflow graph to ReactFlow format
  const syncGraphFromWorker = useCallback((reflowGraph: any) => {
    const reactFlowNodes: Node[] = [];
    const reactFlowEdges: Edge[] = [];

    // Convert Reflow processes to ReactFlow nodes
    if (reflowGraph.processes) {
      Array.from(reflowGraph.processes.values()).forEach((process: any) => {
        const metadata = Object.fromEntries(process.metadata);
        const position = Object.fromEntries(metadata.position || new Map());
        
        reactFlowNodes.push({
          id: process.id,
          type: 'reflow',
          position: position,
          data: {
            label: metadata.name || process.component,
            process: process.component,
            inports: metadata.inports || [],
            outports: metadata.outports || [],
          },
        });
      });
    }

    // Convert Reflow connections to ReactFlow edges
    if (reflowGraph.connections) {
      reflowGraph.connections.forEach((connection: any) => {
        reactFlowEdges.push({
          id: `${connection.from.node_id}-${connection.from.port_name}-to-${connection.to.node_id}-${connection.to.port_name}`,
          source: connection.from.node_id,
          target: connection.to.node_id,
          sourceHandle: connection.from.port_name,
          targetHandle: connection.to.port_name,
        });
      });
    }

    setNodes(reactFlowNodes);
    setEdges(reactFlowEdges);
  }, [setNodes, setEdges]);

  // Functions to sync ReactFlow changes to worker
  const syncToWorker = {
    addNode: useCallback((nodeData: any) => {
      worker.sendMessage({
        type: 'ADD_NODE',
        payload: nodeData
      });
    }, [worker]),

    addEdge: useCallback((edgeData: any) => {
      worker.sendMessage({
        type: 'ADD_EDGE',
        payload: edgeData
      });
    }, [worker]),

    updateNode: useCallback((nodeData: any) => {
      worker.sendMessage({
        type: 'UPDATE_NODE',
        payload: nodeData
      });
    }, [worker]),
  };

  return {
    syncToWorker,
    syncFromWorker: syncGraphFromWorker,
  };
}
```

## Advanced Features

### 1. Component Palette

```typescript
// src/components/Palette/ComponentPalette.tsx
import React from 'react';

const COMPONENT_CATEGORIES = {
  'Data Sources': [
    { type: 'DataSource', label: 'Data Source', description: 'Generate or load data' },
    { type: 'FileReader', label: 'File Reader', description: 'Read files from disk' },
    { type: 'APISource', label: 'API Source', description: 'Fetch data from REST APIs' },
  ],
  'Processors': [
    { type: 'MapActor', label: 'Map', description: 'Transform data elements' },
    { type: 'FilterActor', label: 'Filter', description: 'Filter data elements' },
    { type: 'ReduceActor', label: 'Reduce', description: 'Aggregate data' },
    { type: 'SortActor', label: 'Sort', description: 'Sort data elements' },
  ],
  'Outputs': [
    { type: 'Logger', label: 'Logger', description: 'Log data to console' },
    { type: 'FileWriter', label: 'File Writer', description: 'Write data to file' },
    { type: 'ChartDisplay', label: 'Chart Display', description: 'Visualize data' },
  ],
};

export function ComponentPalette() {
  const onDragStart = (event: React.DragEvent, nodeType: string) => {
    event.dataTransfer.setData('application/reactflow', nodeType);
    event.dataTransfer.effectAllowed = 'move';
  };

  return (
    <div className="component-palette">
      <div className="palette-header">
        <h3>Components</h3>
      </div>
      
      <div className="palette-content">
        {Object.entries(COMPONENT_CATEGORIES).map(([category, components]) => (
          <div key={category} className="component-category">
            <h4>{category}</h4>
            <div className="component-list">
              {components.map((component) => (
                <div
                  key={component.type}
                  className="component-item"
                  draggable
                  onDragStart={(event) => onDragStart(event, component.type)}
                >
                  <div className="component-label">{component.label}</div>
                  <div className="component-description">{component.description}</div>
                </div>
              ))}
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
```

### 2. Execution Controls

```typescript
// src/components/Controls/ExecutionControls.tsx
import React, { useState } from 'react';

interface ExecutionControlsProps {
  onExecute: () => void;
  isReady: boolean;
}

export function ExecutionControls({ onExecute, isReady }: ExecutionControlsProps) {
  const [isExecuting, setIsExecuting] = useState(false);

  const handleExecute = async () => {
    setIsExecuting(true);
    try {
      onExecute();
      // You can add execution status monitoring here
      setTimeout(() => setIsExecuting(false), 2000); // Simulate execution time
    } catch (error) {
      console.error('Execution failed:', error);
      setIsExecuting(false);
    }
  };

  return (
    <div className="execution-controls">
      <button
        onClick={handleExecute}
        disabled={!isReady || isExecuting}
        className={`execute-button ${isExecuting ? 'executing' : ''}`}
      >
        {isExecuting ? 'Executing...' : 'Execute Workflow'}
      </button>
      
      <div className="status-indicator">
        <div className={`status-dot ${isReady ? 'ready' : 'not-ready'}`} />
        <span>{isReady ? 'Ready' : 'Initializing...'}</span>
      </div>
    </div>
  );
}
```

## Complete Example

### 1. Main App Component

```typescript
// src/App.tsx
import React from 'react';
import { ReactFlowProvider } from 'reactflow';
import { ReactFlowEditor } from './components/Editor/ReactFlowEditor';

import './App.css';

function App() {
  return (
    <div className="App">
      <ReactFlowProvider>
        <ReactFlowEditor />
      </ReactFlowProvider>
    </div>
  );
}

export default App;
```

### 2. Complete Styling

```css
/* src/App.css */
.App {
  height: 100vh;
  width: 100vw;
  font-family: -apple-system, BlinkMacSystemFont, 'Segoe UI', 'Roboto', sans-serif;
}

/* Component Palette Styles */
.component-palette {
  width: 300px;
  height: 100vh;
  background: #f8f9fa;
  border-right: 1px solid #e9ecef;
  display: flex;
  flex-direction: column;
  overflow-y: auto;
}

.palette-header {
  padding: 16px;
  background: #ffffff;
  border-bottom: 1px solid #e9ecef;
  box-shadow: 0 1px 3px rgba(0, 0, 0, 0.1);
}

.palette-header h3 {
  margin: 0;
  font-size: 18px;
  font-weight: 600;
  color: #495057;
}

.palette-content {
  flex: 1;
  padding: 16px;
}

.component-category {
  margin-bottom: 24px;
}

.component-category h4 {
  margin: 0 0 12px 0;
  font-size: 14px;
  font-weight: 600;
  color: #6c757d;
  text-transform: uppercase;
  letter-spacing: 0.5px;
}

.component-list {
  display: flex;
  flex-direction: column;
  gap: 8px;
}

.component-item {
  padding: 12px;
  background: #ffffff;
  border: 1px solid #e9ecef;
  border-radius: 8px;
  cursor: grab;
  transition: all 0.2s ease;
  user-select: none;
}

.component-item:hover {
  border-color: #3b82f6;
  box-shadow: 0 2px 8px rgba(59, 130, 246, 0.15);
  transform: translateY(-1px);
}

.component-item:active {
  cursor: grabbing;
  transform: translateY(0);
}

.component-label {
  font-weight: 600;
  color: #212529;
  margin-bottom: 4px;
}

.component-description {
  font-size: 12px;
  color: #6c757d;
  line-height: 1.4;
}

/* Execution Controls Styles */
.execution-controls {
  display: flex;
  flex-direction: column;
  gap: 12px;
  padding: 16px;
  background: #ffffff;
  border: 1px solid #e9ecef;
  border-radius: 8px;
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.1);
  min-width: 200px;
}

.execute-button {
  padding: 10px 16px;
  background: #10b981;
  color: white;
  border: none;
  border-radius: 6px;
  font-weight: 600;
  cursor: pointer;
  transition: all 0.2s ease;
}

.execute-button:hover:not(:disabled) {
  background: #059669;
  transform: translateY(-1px);
}

.execute-button:disabled {
  background: #9ca3af;
  cursor: not-allowed;
  transform: none;
}

.execute-button.executing {
  background: #f59e0b;
  animation: pulse 2s infinite;
}

@keyframes pulse {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.7; }
}

.status-indicator {
  display: flex;
  align-items: center;
  gap: 8px;
  font-size: 14px;
  color: #6c757d;
}

.status-dot {
  width: 8px;
  height: 8px;
  border-radius: 50%;
  transition: background-color 0.3s ease;
}

.status-dot.ready {
  background: #10b981;
}

.status-dot.not-ready {
  background: #ef4444;
  animation: blink 1s infinite;
}

@keyframes blink {
  0%, 100% { opacity: 1; }
  50% { opacity: 0.3; }
}

/* ReactFlow Customizations */
.react-flow__node.react-flow__node-reflow {
  background: transparent;
  border: none;
}

.react-flow__edge-path {
  stroke: #3b82f6;
  stroke-width: 2;
}

.react-flow__edge:hover .react-flow__edge-path {
  stroke: #1d4ed8;
  stroke-width: 3;
}

.react-flow__controls {
  background: #ffffff;
  border: 1px solid #e9ecef;
  border-radius: 8px;
  box-shadow: 0 2px 4px rgba(0, 0, 0, 0.1);
}

.react-flow__controls button {
  background: #ffffff;
  border: none;
  border-bottom: 1px solid #e9ecef;
}

.react-flow__controls button:hover {
  background: #f8f9fa;
}
```

### 3. TypeScript Type Definitions

```typescript
// src/types/reflow.ts
export interface ReflowPort {
  id: string;
  name: string;
  trait: 'data' | 'control' | 'event' | 'config';
  position?: { x: number; y: number };
  metadata?: Record<string, any>;
}

export interface ReflowNodeMetadata {
  position: { x: number; y: number };
  name: string;
  inports: ReflowPort[];
  outports: ReflowPort[];
  [key: string]: any;
}

export interface ReflowConnectionPoint {
  actor: string;
  port: {
    id: string;
    name: string;
    metadata?: Record<string, any>;
  };
}

export interface ReflowConnection {
  from: ReflowConnectionPoint;
  to: ReflowConnectionPoint;
  metadata?: Record<string, any>;
}

export interface ReflowGraphEvent {
  type: 'node_added' | 'edge_added' | 'node_updated' | 'execution_started' | 'execution_completed';
  data: any;
  timestamp: number;
}
```

### 4. Package.json Configuration

```json
{
  "name": "reflow-editor",
  "version": "0.1.0",
  "private": true,
  "dependencies": {
    "@types/react": "^18.2.0",
    "@types/react-dom": "^18.2.0",
    "@types/web": "^0.0.99",
    "react": "^18.2.0",
    "react-dom": "^18.2.0",
    "react-scripts": "5.0.1",
    "reactflow": "^11.10.0",
    "typescript": "^4.9.0"
  },
  "scripts": {
    "start": "react-scripts start",
    "build": "react-scripts build",
    "test": "react-scripts test",
    "eject": "react-scripts eject"
  },
  "eslintConfig": {
    "extends": [
      "react-app",
      "react-app/jest"
    ]
  },
  "browserslist": {
    "production": [
      ">0.2%",
      "not dead",
      "not op_mini all"
    ],
    "development": [
      "last 1 chrome version",
      "last 1 firefox version",
      "last 1 safari version"
    ]
  }
}
```

## Performance Optimization

### 1. Memory Management

```typescript
// Optimize large graphs with virtualization
import { useCallback, useMemo } from 'react';

function useOptimizedNodes(nodes: Node[], viewport: { x: number; y: number; zoom: number }) {
  const visibleNodes = useMemo(() => {
    // Only render nodes in viewport to improve performance
    const padding = 200; // Extra padding around viewport
    const viewportBounds = {
      left: -viewport.x - padding,
      top: -viewport.y - padding,
      right: (-viewport.x + window.innerWidth) / viewport.zoom + padding,
      bottom: (-viewport.y + window.innerHeight) / viewport.zoom + padding,
    };

    return nodes.filter(node => {
      return (
        node.position.x >= viewportBounds.left &&
        node.position.x <= viewportBounds.right &&
        node.position.y >= viewportBounds.top &&
        node.position.y <= viewportBounds.bottom
      );
    });
  }, [nodes, viewport]);

  return visibleNodes;
}
```

### 2. Worker Optimization

```typescript
// Batch worker messages to reduce overhead
class WorkerMessageBatcher {
  private batchedMessages: WorkerMessage[] = [];
  private batchTimeout: NodeJS.Timeout | null = null;
  private worker: Worker;

  constructor(worker: Worker) {
    this.worker = worker;
  }

  sendMessage(message: WorkerMessage) {
    this.batchedMessages.push(message);
    
    if (this.batchTimeout) {
      clearTimeout(this.batchTimeout);
    }

    this.batchTimeout = setTimeout(() => {
      this.flushBatch();
    }, 16); // Batch messages for ~60fps
  }

  private flushBatch() {
    if (this.batchedMessages.length > 0) {
      this.worker.postMessage({
        type: 'BATCH',
        payload: this.batchedMessages
      });
      this.batchedMessages = [];
    }
    this.batchTimeout = null;
  }
}
```

### 3. State Management Optimization

```typescript
// Use React.memo and useMemo for expensive operations
import { memo, useMemo } from 'react';

export const OptimizedReflowNode = memo(({ data, isConnectable }: ReflowNodeProps) => {
  const portColors = useMemo(() => {
    return {
      inports: data.inports.map(port => getPortColor(port.trait)),
      outports: data.outports.map(port => getPortColor(port.trait))
    };
  }, [data.inports, data.outports]);

  return (
    <div className="reflow-node">
      {/* Optimized rendering with memoized colors */}
    </div>
  );
}, (prevProps, nextProps) => {
  // Custom comparison for better performance
  return (
    prevProps.data.label === nextProps.data.label &&
    prevProps.data.process === nextProps.data.process &&
    prevProps.isConnectable === nextProps.isConnectable &&
    JSON.stringify(prevProps.data.inports) === JSON.stringify(nextProps.data.inports) &&
    JSON.stringify(prevProps.data.outports) === JSON.stringify(nextProps.data.outports)
  );
});
```

### 4. WebAssembly Loading Optimization

```typescript
// Pre-load and cache WebAssembly modules
class WasmCache {
  private static instance: WasmCache;
  private wasmModule: WebAssembly.Module | null = null;
  private loading: Promise<WebAssembly.Module> | null = null;

  static getInstance() {
    if (!WasmCache.instance) {
      WasmCache.instance = new WasmCache();
    }
    return WasmCache.instance;
  }

  async getModule(): Promise<WebAssembly.Module> {
    if (this.wasmModule) {
      return this.wasmModule;
    }

    if (this.loading) {
      return this.loading;
    }

    this.loading = this.loadModule();
    this.wasmModule = await this.loading;
    return this.wasmModule;
  }

  private async loadModule(): Promise<WebAssembly.Module> {
    const response = await fetch('/reflow_network_bg.wasm');
    const bytes = await response.arrayBuffer();
    return WebAssembly.compile(bytes);
  }
}
```

## Best Practices & Tips

### 1. Error Handling

- Always wrap worker communication in try-catch blocks
- Implement proper error boundaries in React components
- Provide meaningful error messages to users
- Log errors for debugging but don't expose sensitive information

### 2. State Synchronization

- Keep ReactFlow state as the source of truth for UI
- Use the worker for business logic and persistence
- Implement debouncing for frequent updates
- Handle race conditions in async operations

### 3. Performance

- Use React.memo for components that render frequently
- Implement virtualization for large graphs (>1000 nodes)
- Batch worker messages to reduce overhead
- Optimize WebAssembly loading and initialization

### 4. User Experience

- Show loading states during initialization
- Provide feedback for long-running operations
- Implement undo/redo functionality
- Add keyboard shortcuts for common operations

## Conclusion

This tutorial demonstrated how to build a modern, high-performance visual workflow editor by combining ReactFlow's excellent UI capabilities with Reflow's powerful WebAssembly engine running in a Web Worker.

### Key Benefits Achieved

- **Performance**: UI remains responsive during heavy graph operations
- **Scalability**: Can handle complex workflows with hundreds of nodes
- **Persistence**: Automatic saving and loading of graph state
- **Type Safety**: Full TypeScript integration for better development experience
- **Modularity**: Clean separation between UI and business logic

### Next Steps

- **Custom Components**: Extend the component palette with domain-specific actors
- **Real-time Collaboration**: Add WebSocket support for multi-user editing
- **Advanced Debugging**: Implement step-through execution and breakpoints
- **Plugin System**: Create an extensible architecture for custom functionality
- **Cloud Integration**: Add support for cloud storage and sharing

The architecture presented here provides a solid foundation for building production-ready workflow editors that can scale to enterprise requirements while maintaining excellent user experience.

For more advanced topics and examples, explore the [main Reflow documentation](../README.md) and the [audio-flow example](../../examples/audio-flow/) which demonstrates many of these concepts in action.
