# Browser Actors Guide

Complete guide to creating and managing actors in browser environments using Reflow's WebAssembly bindings.

## Overview

Browser actors in Reflow follow the same conceptual model as native Rust actors but use a JavaScript interface optimized for web environments. They support stateful processing, real-time event handling, and seamless integration with web APIs.

## Actor Lifecycle in Browser

```
┌─────────────────────────────────────────────────────┐
│                Actor Lifecycle                      │
├─────────────────────────────────────────────────────┤
│ 1. Construction                                     │
│    ├─ new MyActor()                                │
│    ├─ Define inports/outports                      │
│    └─ Initialize configuration                     │
├─────────────────────────────────────────────────────┤
│ 2. Registration                                     │
│    ├─ network.registerActor("MyActor", instance)   │
│    └─ WASM bridge creates wrapper                  │
├─────────────────────────────────────────────────────┤
│ 3. Execution                                        │
│    ├─ run(context) called with inputs              │
│    ├─ Access state through context.state           │
│    ├─ Process data with JavaScript logic           │
│    └─ Send outputs via context.send()              │
├─────────────────────────────────────────────────────┤
│ 4. State Persistence                               │
│    ├─ State stored in WASM memory                  │
│    └─ Survives across multiple executions          │
└─────────────────────────────────────────────────────┘
```

## Basic Actor Structure

### Minimal Actor

```javascript
class MinimalActor {
    constructor() {
        // Required: Define input and output ports
        this.inports = ["input"];
        this.outports = ["output"];
        
        // Optional: Actor configuration
        this.config = {};
        
        // State is managed by WASM bridge
        this.state = null;
    }

    /**
     * Main execution method called by the runtime
     * @param {ActorRunContext} context - Execution context
     */
    run(context) {
        // Get input data
        const input = context.input.input;
        
        // Simple processing
        const output = `Processed: ${input}`;
        
        // Send result
        context.send({ output });
    }
}
```

### Stateful Actor

```javascript
class CounterActor {
    constructor() {
        this.inports = ["increment", "reset"];
        this.outports = ["count", "status"];
        this.config = { 
            step: 1,
            maxCount: 100 
        };
    }

    run(context) {
        // Get current count from persistent state
        let count = context.state.get('count') || 0;
        
        // Handle different input ports
        if (context.input.increment !== undefined) {
            count += this.config.step;
            
            // Check bounds
            if (count >= this.config.maxCount) {
                count = this.config.maxCount;
                context.send({ 
                    status: "Maximum count reached" 
                });
            }
            
            // Update state
            context.state.set('count', count);
            
            // Send current count
            context.send({ count });
        }
        
        if (context.input.reset !== undefined) {
            count = 0;
            context.state.set('count', count);
            context.send({ 
                count,
                status: "Counter reset"
            });
        }
    }
}
```

### Configurable Actor

```javascript
class ConfigurableProcessor {
    constructor() {
        this.inports = ["data", "config"];
        this.outports = ["processed", "error"];
        
        // Default configuration
        this.config = {
            mode: "transform",
            batchSize: 1,
            timeout: 5000,
            filters: [],
            outputFormat: "json"
        };
    }

    run(context) {
        // Update configuration if provided
        if (context.input.config) {
            this.updateConfig(context.input.config);
        }
        
        // Process data
        if (context.input.data) {
            try {
                const result = this.processData(context.input.data, context);
                context.send({ processed: result });
            } catch (error) {
                context.send({ 
                    error: {
                        message: error.message,
                        input: context.input.data,
                        timestamp: Date.now()
                    }
                });
            }
        }
    }
    
    updateConfig(newConfig) {
        // Merge with existing configuration
        this.config = { ...this.config, ...newConfig };
        console.log("Updated configuration:", this.config);
    }
    
    processData(data, context) {
        switch (this.config.mode) {
            case "transform":
                return this.transformData(data);
            case "filter":
                return this.filterData(data);
            case "aggregate":
                return this.aggregateData(data, context);
            default:
                throw new Error(`Unknown processing mode: ${this.config.mode}`);
        }
    }
    
    transformData(data) {
        return {
            transformed: true,
            original: data,
            timestamp: Date.now(),
            format: this.config.outputFormat
        };
    }
    
    filterData(data) {
        if (!Array.isArray(data)) {
            data = [data];
        }
        
        return data.filter(item => {
            return this.config.filters.every(filter => 
                this.applyFilter(item, filter)
            );
        });
    }
    
    aggregateData(data, context) {
        // Get previous aggregated data from state
        const previous = context.state.get('aggregated') || [];
        const combined = previous.concat(Array.isArray(data) ? data : [data]);
        
        // Keep only recent data based on batchSize
        const recent = combined.slice(-this.config.batchSize);
        context.state.set('aggregated', recent);
        
        return {
            count: recent.length,
            sum: recent.reduce((acc, val) => acc + (typeof val === 'number' ? val : 0), 0),
            average: recent.length > 0 ? recent.reduce((acc, val) => acc + (typeof val === 'number' ? val : 0), 0) / recent.length : 0,
            latest: recent[recent.length - 1]
        };
    }
    
    applyFilter(item, filter) {
        // Simple filter implementation
        if (filter.field && filter.value) {
            return item[filter.field] === filter.value;
        }
        return true;
    }
}
```

## Advanced Actor Patterns

### Asynchronous Web API Actor

```javascript
class WebAPIActor {
    constructor() {
        this.inports = ["url", "config"];
        this.outports = ["data", "error"];
        this.config = {
            method: "GET",
            timeout: 10000,
            retries: 3
        };
    }

    async run(context) {
        const url = context.input.url;
        const config = { ...this.config, ...context.input.config };
        
        if (!url) {
            context.send({ error: "URL is required" });
            return;
        }

        try {
            const data = await this.fetchWithRetry(url, config);
            context.send({ data });
        } catch (error) {
            context.send({ 
                error: {
                    message: error.message,
                    url,
                    timestamp: Date.now()
                }
            });
        }
    }

    async fetchWithRetry(url, config) {
        let lastError;
        
        for (let attempt = 1; attempt <= config.retries; attempt++) {
            try {
                const controller = new AbortController();
                const timeoutId = setTimeout(() => controller.abort(), config.timeout);
                
                const response = await fetch(url, {
                    method: config.method,
                    headers: config.headers,
                    body: config.body,
                    signal: controller.signal
                });
                
                clearTimeout(timeoutId);
                
                if (!response.ok) {
                    throw new Error(`HTTP ${response.status}: ${response.statusText}`);
                }
                
                return await response.json();
                
            } catch (error) {
                lastError = error;
                
                if (attempt < config.retries) {
                    // Exponential backoff
                    const delay = Math.pow(2, attempt) * 1000;
                    await new Promise(resolve => setTimeout(resolve, delay));
                }
            }
        }
        
        throw lastError;
    }
}
```

### Timer Actor

```javascript
class TimerActor {
    constructor() {
        this.inports = ["start", "stop", "interval"];
        this.outports = ["tick", "status"];
        this.config = { defaultInterval: 1000 };
        
        // Store timer reference
        this.timerId = null;
    }

    run(context) {
        if (context.input.start !== undefined) {
            this.startTimer(context);
        }
        
        if (context.input.stop !== undefined) {
            this.stopTimer(context);
        }
        
        if (context.input.interval !== undefined) {
            this.updateInterval(context.input.interval, context);
        }
    }
    
    startTimer(context) {
        // Stop existing timer if running
        this.stopTimer(context, false);
        
        const interval = context.state.get('interval') || this.config.defaultInterval;
        let tickCount = context.state.get('tickCount') || 0;
        
        this.timerId = setInterval(() => {
            tickCount++;
            context.state.set('tickCount', tickCount);
            
            // Send tick event
            context.send({
                tick: {
                    count: tickCount,
                    timestamp: Date.now(),
                    interval: interval
                }
            });
        }, interval);
        
        context.state.set('running', true);
        context.send({ status: `Timer started with ${interval}ms interval` });
    }
    
    stopTimer(context, sendStatus = true) {
        if (this.timerId) {
            clearInterval(this.timerId);
            this.timerId = null;
        }
        
        context.state.set('running', false);
        
        if (sendStatus) {
            const tickCount = context.state.get('tickCount') || 0;
            context.send({ 
                status: `Timer stopped after ${tickCount} ticks` 
            });
        }
    }
    
    updateInterval(newInterval, context) {
        context.state.set('interval', newInterval);
        
        // Restart timer with new interval if currently running
        if (context.state.get('running')) {
            this.startTimer(context);
        }
    }
}
```

### File Reader Actor (Browser)

```javascript
class FileReaderActor {
    constructor() {
        this.inports = ["file", "options"];
        this.outports = ["content", "progress", "error"];
        this.config = {
            readAs: "text", // "text", "dataURL", "arrayBuffer"
            encoding: "utf-8",
            chunkSize: 64 * 1024 // 64KB chunks for progress
        };
    }

    run(context) {
        const file = context.input.file;
        const options = { ...this.config, ...context.input.options };
        
        if (!file || !file instanceof File) {
            context.send({ error: "Valid File object required" });
            return;
        }

        this.readFile(file, options, context);
    }

    readFile(file, options, context) {
        const reader = new FileReader();
        
        // Track progress
        reader.onprogress = (event) => {
            if (event.lengthComputable) {
                const progress = (event.loaded / event.total) * 100;
                context.send({ 
                    progress: {
                        loaded: event.loaded,
                        total: event.total,
                        percentage: progress
                    }
                });
            }
        };
        
        reader.onload = (event) => {
            const result = event.target.result;
            context.send({
                content: {
                    data: result,
                    filename: file.name,
                    size: file.size,
                    type: file.type,
                    lastModified: file.lastModified,
                    readAs: options.readAs
                }
            });
        };
        
        reader.onerror = (event) => {
            context.send({
                error: {
                    message: "Failed to read file",
                    filename: file.name,
                    error: event.target.error
                }
            });
        };

        // Choose reading method based on options
        switch (options.readAs) {
            case "text":
                reader.readAsText(file, options.encoding);
                break;
            case "dataURL":
                reader.readAsDataURL(file);
                break;
            case "arrayBuffer":
                reader.readAsArrayBuffer(file);
                break;
            default:
                context.send({ error: `Unsupported read method: ${options.readAs}` });
        }
    }
}
```

## State Management Patterns

### Complex State Actor

```javascript
class StatefulProcessor {
    constructor() {
        this.inports = ["data", "command"];
        this.outports = ["result", "state", "error"];
        this.config = {};
    }

    run(context) {
        // Handle commands
        if (context.input.command) {
            this.handleCommand(context.input.command, context);
        }
        
        // Process data
        if (context.input.data) {
            this.processData(context.input.data, context);
        }
    }
    
    handleCommand(command, context) {
        switch (command.action) {
            case "get_state":
                context.send({ 
                    state: context.state.getAll() 
                });
                break;
                
            case "set_state":
                if (command.data) {
                    context.state.setAll(command.data);
                    context.send({ 
                        result: "State updated successfully" 
                    });
                }
                break;
                
            case "clear_state":
                context.state.clear();
                context.send({ 
                    result: "State cleared" 
                });
                break;
                
            case "get_stats":
                this.sendStatistics(context);
                break;
                
            default:
                context.send({ 
                    error: `Unknown command: ${command.action}` 
                });
        }
    }
    
    processData(data, context) {
        // Update processing statistics
        const stats = context.state.get('stats') || {
            processedCount: 0,
            totalSize: 0,
            lastProcessed: null,
            errors: 0
        };
        
        try {
            // Simulate processing
            const processed = this.transform(data);
            
            // Update statistics
            stats.processedCount++;
            stats.totalSize += JSON.stringify(data).length;
            stats.lastProcessed = Date.now();
            
            context.state.set('stats', stats);
            context.send({ result: processed });
            
        } catch (error) {
            stats.errors++;
            context.state.set('stats', stats);
            
            context.send({
                error: {
                    message: error.message,
                    data: data,
                    timestamp: Date.now()
                }
            });
        }
    }
    
    transform(data) {
        return {
            original: data,
            transformed: Array.isArray(data) ? data.map(x => x * 2) : data,
            timestamp: Date.now()
        };
    }
    
    sendStatistics(context) {
        const stats = context.state.get('stats') || {};
        const stateSize = context.state.size();
        
        context.send({
            state: {
                statistics: stats,
                stateSize: stateSize,
                stateKeys: context.state.keys(),
                uptime: Date.now() - (stats.firstProcessed || Date.now())
            }
        });
    }
}
```

### Cache Actor

```javascript
class CacheActor {
    constructor() {
        this.inports = ["get", "set", "delete", "clear"];
        this.outports = ["value", "status", "stats"];
        this.config = {
            maxSize: 100,
            ttlMs: 300000 // 5 minutes
        };
    }

    run(context) {
        if (context.input.get) {
            this.getValue(context.input.get, context);
        }
        
        if (context.input.set) {
            this.setValue(context.input.set, context);
        }
        
        if (context.input.delete) {
            this.deleteValue(context.input.delete, context);
        }
        
        if (context.input.clear) {
            this.clearCache(context);
        }
    }
    
    getValue(request, context) {
        const cache = context.state.get('cache') || {};
        const entry = cache[request.key];
        
        if (!entry) {
            context.send({ 
                value: { 
                    key: request.key, 
                    found: false 
                } 
            });
            return;
        }
        
        // Check TTL
        if (entry.expires && Date.now() > entry.expires) {
            delete cache[request.key];
            context.state.set('cache', cache);
            
            context.send({ 
                value: { 
                    key: request.key, 
                    found: false, 
                    expired: true 
                } 
            });
            return;
        }
        
        // Update access time
        entry.lastAccessed = Date.now();
        context.state.set('cache', cache);
        
        context.send({
            value: {
                key: request.key,
                value: entry.value,
                found: true,
                created: entry.created,
                lastAccessed: entry.lastAccessed
            }
        });
    }
    
    setValue(request, context) {
        const cache = context.state.get('cache') || {};
        
        // Enforce size limit
        const keys = Object.keys(cache);
        if (keys.length >= this.config.maxSize && !cache[request.key]) {
            // Remove oldest entry
            const oldest = keys.reduce((min, key) => 
                (!min || cache[key].lastAccessed < cache[min].lastAccessed) ? key : min
            );
            delete cache[oldest];
        }
        
        // Set new value
        const now = Date.now();
        cache[request.key] = {
            value: request.value,
            created: now,
            lastAccessed: now,
            expires: request.ttl ? now + request.ttl : now + this.config.ttlMs
        };
        
        context.state.set('cache', cache);
        
        context.send({
            status: {
                operation: "set",
                key: request.key,
                success: true,
                cacheSize: Object.keys(cache).length
            }
        });
    }
    
    deleteValue(request, context) {
        const cache = context.state.get('cache') || {};
        const existed = cache[request.key] !== undefined;
        
        delete cache[request.key];
        context.state.set('cache', cache);
        
        context.send({
            status: {
                operation: "delete",
                key: request.key,
                existed: existed,
                cacheSize: Object.keys(cache).length
            }
        });
    }
    
    clearCache(context) {
        const cache = context.state.get('cache') || {};
        const count = Object.keys(cache).length;
        
        context.state.set('cache', {});
        
        context.send({
            status: {
                operation: "clear",
                clearedCount: count,
                cacheSize: 0
            }
        });
    }
}
```

## Integration with Browser APIs

### Geolocation Actor

```javascript
class GeolocationActor {
    constructor() {
        this.inports = ["getCurrentPosition", "watchPosition", "clearWatch"];
        this.outports = ["position", "error"];
        this.config = {
            enableHighAccuracy: false,
            timeout: 10000,
            maximumAge: 600000 // 10 minutes
        };
        
        this.watchId = null;
    }

    run(context) {
        if (!navigator.geolocation) {
            context.send({ error: "Geolocation is not supported" });
            return;
        }
        
        if (context.input.getCurrentPosition) {
            this.getCurrentPosition(context);
        }
        
        if (context.input.watchPosition) {
            this.startWatching(context);
        }
        
        if (context.input.clearWatch) {
            this.stopWatching(context);
        }
    }
    
    getCurrentPosition(context) {
        const options = { ...this.config, ...context.input.getCurrentPosition };
        
        navigator.geolocation.getCurrentPosition(
            (position) => {
                context.send({
                    position: {
                        latitude: position.coords.latitude,
                        longitude: position.coords.longitude,
                        accuracy: position.coords.accuracy,
                        altitude: position.coords.altitude,
                        heading: position.coords.heading,
                        speed: position.coords.speed,
                        timestamp: position.timestamp
                    }
                });
            },
            (error) => {
                context.send({
                    error: {
                        code: error.code,
                        message: error.message,
                        timestamp: Date.now()
                    }
                });
            },
            options
        );
    }
    
    startWatching(context) {
        this.stopWatching(context, false);
        
        const options = { ...this.config, ...context.input.watchPosition };
        
        this.watchId = navigator.geolocation.watchPosition(
            (position) => {
                context.send({
                    position: {
                        latitude: position.coords.latitude,
                        longitude: position.coords.longitude,
                        accuracy: position.coords.accuracy,
                        altitude: position.coords.altitude,
                        heading: position.coords.heading,
                        speed: position.coords.speed,
                        timestamp: position.timestamp,
                        isWatching: true
                    }
                });
            },
            (error) => {
                context.send({
                    error: {
                        code: error.code,
                        message: error.message,
                        timestamp: Date.now(),
                        isWatching: true
                    }
                });
            },
            options
        );
        
        context.state.set('watching', true);
    }
    
    stopWatching(context, sendConfirmation = true) {
        if (this.watchId !== null) {
            navigator.geolocation.clearWatch(this.watchId);
            this.watchId = null;
        }
        
        context.state.set('watching', false);
        
        if (sendConfirmation) {
            context.send({
                position: {
                    message: "Stopped watching position",
                    timestamp: Date.now(),
                    isWatching: false
                }
            });
        }
    }
}
```

## Testing and Debugging Actors

### Test Helper Functions

```javascript
// Actor testing utilities
class ActorTester {
    constructor(ActorClass) {
        this.ActorClass = ActorClass;
        this.actor = new ActorClass();
        this.mockState = new Map();
        this.outputs = [];
    }
    
    // Create a mock context for testing
    createMockContext(inputs) {
        const self = this;
        
        return {
            input: inputs,
            state: {
                get: (key) => self.mockState.get(key),
                set: (key, value) => self.mockState.set(key, value),
                has: (key) => self.mockState.has(key),
                remove: (key) => self.mockState.delete(key),
                clear: () => self.mockState.clear(),
                getAll: () => Object.fromEntries(self.mockState),
                setAll: (obj) => {
                    self.mockState.clear();
                    Object.entries(obj).forEach(([k, v]) => self.mockState.set(k, v));
                },
                size: () => self.mockState.size,
                keys: () => Array.from(self.mockState.keys()),
                values: () => Array.from(self.mockState.values())
            },
            send: (outputs) => {
                self.outputs.push({
                    timestamp: Date.now(),
                    outputs: outputs
                });
            }
        };
    }
    
    // Test actor with given inputs
    test(inputs, expectedOutputs) {
        this.outputs = [];
        const context = this.createMockContext(inputs);
        
        // Run the actor
        const result = this.actor.run(context);
        
        // Handle async actors
        if (result instanceof Promise) {
            return result.then(() => this.verifyOutputs(expectedOutputs));
        } else {
            return this.verifyOutputs(expectedOutputs);
        }
    }
    
    verifyOutputs(expectedOutputs) {
        const results = {
            passed: true,
            outputs: this.outputs,
            state: Object.fromEntries(this.mockState),
            errors: []
        };
        
        if (expectedOutputs) {
            // Simple verification - can be enhanced
            if (this.outputs.length !== expectedOutputs.length) {
                results.passed = false;
                results.errors.push(`Expected ${expectedOutputs.length} outputs, got ${this.outputs.length}`);
            }
        }
        
        return results;
    }
}

// Example usage
async function testCounterActor() {
    const tester = new ActorTester(CounterActor);
    
    // Test increment
    const result1 = await tester.test({ increment: 1 });
    console.log("Increment test:", result1);
    
    // Test reset
    const result2 = await tester.test({ reset: true });
    console.log("Reset test:", result2);
}
```

### Debug Actor Wrapper

```javascript
class DebugActorWrapper {
    constructor(actor, name) {
        this.actor = actor;
        this.name = name || actor.constructor.name;
        this.executionCount = 0;
        this.totalExecutionTime = 0;
    }
    
    get inports() { return this.actor.inports; }
    get outports() { return this.actor.outports; }
    get config() { return this.actor.config; }
    set config(value) { this.actor.config = value; }
    
    run(context) {
        this.executionCount++;
        const startTime = performance.now();
        
        console.group(`🎭 ${this.name} #${this.executionCount}`);
        console.log("Inputs:", context.input);
        console.log("State before:", context.state.getAll());
        
        // Wrap the send method to log outputs
        const originalSend = context.send;
        context.send = (outputs) => {
            console.log("Outputs:", outputs);
            originalSend(outputs);
        };
        
        try {
            const result = this.actor.run(context);
            
            const endTime = performance.now();
            const executionTime = endTime - startTime;
            this.totalExecutionTime += executionTime;
            
            console.log("State after:", context.state.getAll());
            console.log(`Execution time: ${executionTime.toFixed(2)}ms`);
            console.log(`Average time: ${(this.totalExecutionTime / this.executionCount).toFixed(2)}ms`);
            console.groupEnd();
            
            return result;
            
        } catch (error) {
            console.error("Actor error:", error);
            console.groupEnd();
            throw error;
        }
    }
}

// Usage
const debugCounter = new DebugActorWrapper(new CounterActor(), "MyCounter");
network.registerActor("CounterActor", debugCounter);
```

## Performance Optimization

### Efficient Actor Patterns

```javascript
// ✅ Good: Minimal state operations
class EfficientActor {
    run(context) {
        // Read state once
        const state = context.state.getAll();
        
        // Modify locally
        state.counter = (state.counter || 0) + 1;
        state.lastUpdate = Date.now();
        
        // Write once
        context.state.setAll(state);
        
        context.send({ output: state.counter });
    }
}

// ❌ Avoid: Multiple state operations
class InefficientActor {
    run(context) {
        // Multiple gets/sets are slower
        const counter = context.state.get('counter') || 0;
        context.state.set('counter', counter + 1);
        
        const lastUpdate = Date.now();
        context.state.set('lastUpdate', lastUpdate);
        
        context.send({ output: counter + 1 });
    }
}

// ✅ Good: Batch processing
class BatchActor {
    constructor() {
        this.inports = ["input"];
        this.outports = ["output"];
        this.config = { batchSize: 10 };
    }
    
    run(context) {
        const batch = context.state.get('batch') || [];
        batch.push(context.input.input);
        
        if (batch.length >= this.config.batchSize) {
            // Process entire batch at once
            const results = this.processBatch(batch);
            context.send({ output: results });
            context.state.set('batch', []);
        } else {
            context.state.set('batch', batch);
        }
    }
    
    processBatch(items) {
        return items.map(item => ({ processed: item, timestamp: Date.now() }));
    }
}
```

## Next Steps

- **[Graph Management](graphs-and-networks.md)** - Creating and managing graphs in browser
- **[State Management](state-management.md)** - Advanced state handling patterns
- **[Events & Monitoring](events-and-monitoring.md)** - Real-time event handling
- **[Browser Workflow Editor Tutorial](../../tutorials/browser-workflow-editor.md)** - Building visual editors

Browser actors provide a powerful way to create interactive, stateful workflows that run entirely in the browser. Use the patterns and examples above to build robust, performant actor-based applications.
