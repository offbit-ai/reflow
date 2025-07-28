# Browser Actor Development Tutorial

Learn how to develop actors specifically for browser environments using Reflow's WebAssembly bindings.

## Overview

This tutorial covers creating actors that leverage browser APIs, handle asynchronous operations, and provide interactive user experiences. We'll build several example actors that demonstrate common patterns in browser-based workflow development.

## Prerequisites

- Basic understanding of JavaScript and async programming
- Familiarity with [Reflow's actor concepts](../getting-started/basic-concepts.md)
- [Browser setup completed](../deployment/browser-deployment.md)

## Tutorial Structure

We'll build increasingly complex actors:

1. **Data Transformation Actor** - Basic processing patterns
2. **Web API Client Actor** - HTTP requests and async operations
3. **File Processing Actor** - Browser file handling
4. **Real-time Data Actor** - WebSocket connections and streaming
5. **Interactive UI Actor** - DOM manipulation and user interaction

## 1. Data Transformation Actor

Let's start with a versatile data transformation actor that handles various data formats.

### Creating the Actor

```javascript
class DataTransformActor {
    constructor() {
        this.inports = ["data", "config"];
        this.outports = ["result", "error", "stats"];
        
        // Default transformation configuration
        this.config = {
            operation: "normalize",  // normalize, aggregate, filter, map
            outputFormat: "json",    // json, csv, xml
            precision: 2,           // for numeric operations
            batchSize: 100          // for batch processing
        };
    }

    run(context) {
        // Update configuration if provided
        if (context.input.config) {
            this.updateConfig(context.input.config);
        }

        // Process incoming data
        if (context.input.data !== undefined) {
            try {
                const result = this.transform(context.input.data, context);
                this.updateStats(context, result);
                context.send({ result });
            } catch (error) {
                this.handleError(error, context);
            }
        }
    }

    updateConfig(newConfig) {
        this.config = { ...this.config, ...newConfig };
        console.log("Configuration updated:", this.config);
    }

    transform(data, context) {
        switch (this.config.operation) {
            case "normalize":
                return this.normalizeData(data);
            case "aggregate": 
                return this.aggregateData(data, context);
            case "filter":
                return this.filterData(data);
            case "map":
                return this.mapData(data);
            default:
                throw new Error(`Unknown operation: ${this.config.operation}`);
        }
    }

    normalizeData(data) {
        if (!Array.isArray(data)) {
            data = [data];
        }

        return data.map(item => {
            if (typeof item === 'number') {
                return Number(item.toFixed(this.config.precision));
            }
            
            if (typeof item === 'string') {
                return item.trim().toLowerCase();
            }
            
            if (typeof item === 'object' && item !== null) {
                const normalized = {};
                for (const [key, value] of Object.entries(item)) {
                    // Normalize keys to camelCase
                    const normalizedKey = key.replace(/_([a-z])/g, (g) => g[1].toUpperCase());
                    normalized[normalizedKey] = typeof value === 'string' ? 
                        value.trim() : value;
                }
                return normalized;
            }
            
            return item;
        });
    }

    aggregateData(data, context) {
        if (!Array.isArray(data)) {
            data = [data];
        }

        // Get existing aggregation state
        const aggregated = context.state.get('aggregated') || [];
        const combined = aggregated.concat(data);
        
        // Keep only recent data based on batch size
        const recent = combined.slice(-this.config.batchSize);
        context.state.set('aggregated', recent);

        // Calculate statistics
        const numbers = recent.filter(x => typeof x === 'number');
        const result = {
            count: recent.length,
            numericCount: numbers.length,
            sum: numbers.reduce((a, b) => a + b, 0),
            average: numbers.length > 0 ? 
                numbers.reduce((a, b) => a + b, 0) / numbers.length : 0,
            min: numbers.length > 0 ? Math.min(...numbers) : null,
            max: numbers.length > 0 ? Math.max(...numbers) : null,
            latest: recent[recent.length - 1],
            timestamp: Date.now()
        };

        return result;
    }

    filterData(data) {
        if (!Array.isArray(data)) {
            return data;
        }

        // Apply various filters based on configuration
        return data.filter(item => {
            // Filter out null/undefined
            if (item == null) return false;
            
            // Filter by type if specified
            if (this.config.filterType) {
                if (typeof item !== this.config.filterType) return false;
            }
            
            // Filter by value range for numbers
            if (typeof item === 'number') {
                if (this.config.minValue !== undefined && item < this.config.minValue) return false;
                if (this.config.maxValue !== undefined && item > this.config.maxValue) return false;
            }
            
            // Filter by pattern for strings
            if (typeof item === 'string' && this.config.pattern) {
                const regex = new RegExp(this.config.pattern, 'i');
                if (!regex.test(item)) return false;
            }
            
            return true;
        });
    }

    mapData(data) {
        if (!Array.isArray(data)) {
            data = [data];
        }

        return data.map((item, index) => {
            const mapped = {
                original: item,
                index: index,
                timestamp: Date.now(),
                processed: true
            };

            // Apply transformations based on type
            if (typeof item === 'number') {
                mapped.doubled = item * 2;
                mapped.squared = item * item;
                mapped.formatted = item.toFixed(this.config.precision);
            }
            
            if (typeof item === 'string') {
                mapped.length = item.length;
                mapped.uppercase = item.toUpperCase();
                mapped.words = item.split(/\s+/).length;
            }
            
            if (typeof item === 'object' && item !== null) {
                mapped.keys = Object.keys(item);
                mapped.keyCount = Object.keys(item).length;
            }

            return mapped;
        });
    }

    updateStats(context, result) {
        const stats = context.state.get('stats') || {
            processedCount: 0,
            lastProcessed: null,
            totalDataSize: 0,
            operationCounts: {}
        };

        stats.processedCount++;
        stats.lastProcessed = Date.now();
        stats.totalDataSize += JSON.stringify(result).length;
        
        const operation = this.config.operation;
        stats.operationCounts[operation] = (stats.operationCounts[operation] || 0) + 1;

        context.state.set('stats', stats);
        
        // Send stats periodically
        if (stats.processedCount % 10 === 0) {
            context.send({ stats });
        }
    }

    handleError(error, context) {
        const errorInfo = {
            message: error.message,
            operation: this.config.operation,
            timestamp: Date.now(),
            config: this.config
        };

        console.error("DataTransformActor error:", errorInfo);
        context.send({ error: errorInfo });
    }
}
```

### Testing the Transform Actor

```javascript
// Test the data transformation actor
async function testDataTransformActor() {
    const graph = new Graph("DataTransformTest", true);
    
    // Add the transform actor
    graph.addNode("transformer", "DataTransformActor", {
        x: 200, y: 100,
        description: "Data transformation processor"
    });

    // Add test data
    const testData = [
        1.23456, 2.67890, 3.14159,
        "  Hello World  ", "  JAVASCRIPT  ",
        { first_name: "John", last_name: "Doe", age: 30 },
        { product_id: 123, product_name: "Widget", price: 9.99 }
    ];

    graph.addInitial(testData, "transformer", "data");
    
    // Test different operations
    const operations = [
        { operation: "normalize", precision: 2 },
        { operation: "aggregate", batchSize: 50 },
        { operation: "filter", filterType: "number", minValue: 2 },
        { operation: "map" }
    ];

    const network = new GraphNetwork(graph);
    network.registerActor("DataTransformActor", new DataTransformActor());

    // Monitor results
    network.next((event) => {
        if (event._type === "FlowTrace" && event.to.port === "result") {
            console.log("Transform result:", event.from.data);
        }
    });

    await network.start();

    // Test different operations
    for (const config of operations) {
        console.log(`\nTesting operation: ${config.operation}`);
        const result = await network.executeActor("transformer", {
            data: testData,
            config: config
        });
        console.log("Result:", result);
    }
}
```

## 2. Web API Client Actor

Now let's build an actor that makes HTTP requests and handles various web APIs.

### Creating the API Client Actor

```javascript
class WebAPIClientActor {
    constructor() {
        this.inports = ["request", "config"];
        this.outports = ["response", "error", "progress"];
        
        this.config = {
            baseURL: "",
            timeout: 10000,
            retries: 3,
            retryDelay: 1000,
            headers: {
                "Content-Type": "application/json"
            }
        };
    }

    async run(context) {
        // Update configuration
        if (context.input.config) {
            this.config = { ...this.config, ...context.input.config };
        }

        // Process request
        if (context.input.request) {
            await this.makeRequest(context.input.request, context);
        }
    }

    async makeRequest(request, context) {
        const url = this.buildURL(request.url);
        const options = this.buildRequestOptions(request);
        
        try {
            const response = await this.fetchWithRetry(url, options, context);
            const data = await this.parseResponse(response, request.responseType);
            
            context.send({
                response: {
                    data: data,
                    status: response.status,
                    statusText: response.statusText,
                    headers: Object.fromEntries(response.headers.entries()),
                    url: response.url,
                    timestamp: Date.now()
                }
            });

        } catch (error) {
            context.send({
                error: {
                    message: error.message,
                    type: error.name,
                    url: url,
                    request: request,
                    timestamp: Date.now()
                }
            });
        }
    }

    buildURL(url) {
        if (url.startsWith('http')) {
            return url;
        }
        return `${this.config.baseURL}${url}`;
    }

    buildRequestOptions(request) {
        const options = {
            method: request.method || 'GET',
            headers: { ...this.config.headers, ...request.headers }
        };

        if (request.body) {
            if (typeof request.body === 'object') {
                options.body = JSON.stringify(request.body);
            } else {
                options.body = request.body;
            }
        }

        return options;
    }

    async fetchWithRetry(url, options, context) {
        let lastError;
        
        for (let attempt = 1; attempt <= this.config.retries; attempt++) {
            try {
                // Set up timeout
                const controller = new AbortController();
                const timeoutId = setTimeout(() => controller.abort(), this.config.timeout);
                
                options.signal = controller.signal;

                // Send progress update
                context.send({
                    progress: {
                        attempt: attempt,
                        maxAttempts: this.config.retries,
                        url: url,
                        status: "requesting"
                    }
                });

                const response = await fetch(url, options);
                clearTimeout(timeoutId);

                if (!response.ok) {
                    throw new Error(`HTTP ${response.status}: ${response.statusText}`);
                }

                return response;

            } catch (error) {
                lastError = error;
                
                if (attempt < this.config.retries) {
                    const delay = this.config.retryDelay * Math.pow(2, attempt - 1);
                    
                    context.send({
                        progress: {
                            attempt: attempt,
                            maxAttempts: this.config.retries,
                            error: error.message,
                            retryIn: delay,
                            status: "retrying"
                        }
                    });

                    await new Promise(resolve => setTimeout(resolve, delay));
                }
            }
        }
        
        throw lastError;
    }

    async parseResponse(response, responseType = 'json') {
        switch (responseType.toLowerCase()) {
            case 'json':
                return await response.json();
            case 'text':
                return await response.text();
            case 'blob':
                return await response.blob();
            case 'arraybuffer':
                return await response.arrayBuffer();
            default:
                return await response.json();
        }
    }
}
```

### API Actor Examples

```javascript
// Example: Weather API actor
class WeatherAPIActor extends WebAPIClientActor {
    constructor() {
        super();
        this.inports = ["location", "config"];
        this.outports = ["weather", "error"];
        
        this.config = {
            ...this.config,
            baseURL: "https://api.openweathermap.org/data/2.5",
            apiKey: "" // Set your API key
        };
    }

    async run(context) {
        if (context.input.location) {
            const request = {
                url: `/weather?q=${encodeURIComponent(context.input.location)}&appid=${this.config.apiKey}&units=metric`,
                method: "GET",
                responseType: "json"
            };

            await this.makeRequest(request, context);
        }
    }
}

// Example: REST API actor
class RESTAPIActor extends WebAPIClientActor {
    constructor() {
        super();
        this.inports = ["operation", "config"];
        this.outports = ["result", "error"];
    }

    async run(context) {
        if (context.input.config) {
            this.config = { ...this.config, ...context.input.config };
        }

        if (context.input.operation) {
            const op = context.input.operation;
            const request = {
                url: op.endpoint,
                method: op.method || 'GET',
                headers: op.headers,
                body: op.data,
                responseType: op.responseType || 'json'
            };

            await this.makeRequest(request, context);
        }
    }
}
```

## 3. File Processing Actor

Let's create an actor that handles file operations in the browser.

```javascript
class FileProcessorActor {
    constructor() {
        this.inports = ["file", "operation", "config"];
        this.outports = ["content", "metadata", "progress", "error"];
        
        this.config = {
            chunkSize: 64 * 1024, // 64KB chunks
            supportedTypes: ['text', 'json', 'csv', 'xml'],
            maxFileSize: 10 * 1024 * 1024 // 10MB
        };
    }

    run(context) {
        if (context.input.config) {
            this.config = { ...this.config, ...context.input.config };
        }

        if (context.input.file && context.input.operation) {
            this.processFile(context.input.file, context.input.operation, context);
        }
    }

    processFile(file, operation, context) {
        // Validate file
        if (!file || !(file instanceof File)) {
            context.send({ error: "Valid File object required" });
            return;
        }

        if (file.size > this.config.maxFileSize) {
            context.send({ 
                error: `File too large: ${file.size} bytes (max: ${this.config.maxFileSize})` 
            });
            return;
        }

        // Send file metadata
        context.send({
            metadata: {
                name: file.name,
                size: file.size,
                type: file.type,
                lastModified: new Date(file.lastModified),
                operation: operation
            }
        });

        // Process based on operation
        switch (operation.type) {
            case 'read':
                this.readFile(file, operation, context);
                break;
            case 'parse':
                this.parseFile(file, operation, context);
                break;
            case 'analyze':
                this.analyzeFile(file, operation, context);
                break;
            default:
                context.send({ error: `Unknown operation: ${operation.type}` });
        }
    }

    readFile(file, operation, context) {
        const reader = new FileReader();
        
        reader.onprogress = (event) => {
            if (event.lengthComputable) {
                context.send({
                    progress: {
                        loaded: event.loaded,
                        total: event.total,
                        percentage: (event.loaded / event.total) * 100,
                        operation: 'reading'
                    }
                });
            }
        };

        reader.onload = (event) => {
            const result = event.target.result;
            context.send({
                content: {
                    data: result,
                    encoding: operation.encoding || 'utf-8',
                    format: operation.format || 'text',
                    size: result.length || result.byteLength
                }
            });
        };

        reader.onerror = () => {
            context.send({
                error: `Failed to read file: ${reader.error.message}`
            });
        };

        // Choose reading method
        const format = operation.format || 'text';
        switch (format) {
            case 'text':
                reader.readAsText(file, operation.encoding || 'utf-8');
                break;
            case 'dataurl':
                reader.readAsDataURL(file);
                break;
            case 'binary':
                reader.readAsArrayBuffer(file);
                break;
            default:
                reader.readAsText(file);
        }
    }

    parseFile(file, operation, context) {
        const reader = new FileReader();
        
        reader.onload = (event) => {
            try {
                const text = event.target.result;
                let parsed;

                switch (operation.format) {
                    case 'json':
                        parsed = JSON.parse(text);
                        break;
                    case 'csv':
                        parsed = this.parseCSV(text);
                        break;
                    case 'xml':
                        parsed = this.parseXML(text);
                        break;
                    default:
                        parsed = text;
                }

                context.send({
                    content: {
                        data: parsed,
                        format: operation.format,
                        recordCount: Array.isArray(parsed) ? parsed.length : 1
                    }
                });

            } catch (error) {
                context.send({
                    error: `Failed to parse ${operation.format}: ${error.message}`
                });
            }
        };

        reader.readAsText(file);
    }

    parseCSV(text) {
        const lines = text.split('\n').filter(line => line.trim());
        if (lines.length === 0) return [];

        const headers = lines[0].split(',').map(h => h.trim());
        const data = [];

        for (let i = 1; i < lines.length; i++) {
            const values = lines[i].split(',').map(v => v.trim());
            const row = {};
            
            headers.forEach((header, index) => {
                row[header] = values[index] || '';
            });
            
            data.push(row);
        }

        return data;
    }

    parseXML(text) {
        const parser = new DOMParser();
        const doc = parser.parseFromString(text, 'text/xml');
        
        if (doc.querySelector('parsererror')) {
            throw new Error('Invalid XML format');
        }

        return this.xmlToObject(doc.documentElement);
    }

    xmlToObject(element) {
        const obj = {};
        
        // Add attributes
        if (element.attributes.length > 0) {
            obj['@attributes'] = {};
            for (const attr of element.attributes) {
                obj['@attributes'][attr.name] = attr.value;
            }
        }

        // Add child elements
        for (const child of element.children) {
            const name = child.tagName;
            const value = child.children.length > 0 ? 
                this.xmlToObject(child) : child.textContent;
            
            if (obj[name]) {
                if (!Array.isArray(obj[name])) {
                    obj[name] = [obj[name]];
                }
                obj[name].push(value);
            } else {
                obj[name] = value;
            }
        }

        return obj;
    }

    analyzeFile(file, operation, context) {
        const reader = new FileReader();
        
        reader.onload = (event) => {
            const text = event.target.result;
            const analysis = {
                size: text.length,
                lines: text.split('\n').length,
                words: text.split(/\s+/).filter(w => w).length,
                characters: text.length,
                charactersNoSpaces: text.replace(/\s/g, '').length,
                encoding: 'utf-8',
                detectedFormat: this.detectFormat(text, file.name),
                timestamp: Date.now()
            };

            context.send({ content: analysis });
        };

        reader.readAsText(file);
    }

    detectFormat(text, filename) {
        const extension = filename.split('.').pop().toLowerCase();
        
        // Try to detect format
        try {
            JSON.parse(text);
            return 'json';
        } catch {}

        if (text.includes('<?xml') || text.includes('<html')) {
            return 'xml';
        }

        if (text.includes(',') && text.split('\n').length > 1) {
            return 'csv';
        }

        return extension || 'text';
    }
}
```

## 4. Real-time Data Actor

Let's build an actor that handles real-time data streams via WebSockets.

```javascript
class WebSocketActor {
    constructor() {
        this.inports = ["connect", "disconnect", "send", "config"];
        this.outports = ["message", "status", "error"];
        
        this.config = {
            reconnectAttempts: 5,
            reconnectDelay: 1000,
            pingInterval: 30000,
            maxMessageSize: 1024 * 1024 // 1MB
        };
        
        this.socket = null;
        this.reconnectCount = 0;
        this.pingTimer = null;
    }

    run(context) {
        if (context.input.config) {
            this.config = { ...this.config, ...context.input.config };
        }

        if (context.input.connect) {
            this.connect(context.input.connect, context);
        }

        if (context.input.disconnect) {
            this.disconnect(context);
        }

        if (context.input.send) {
            this.sendMessage(context.input.send, context);
        }
    }

    connect(connectionInfo, context) {
        if (this.socket && this.socket.readyState === WebSocket.OPEN) {
            context.send({ status: "Already connected" });
            return;
        }

        try {
            this.socket = new WebSocket(connectionInfo.url, connectionInfo.protocols);
            
            this.socket.onopen = () => {
                this.reconnectCount = 0;
                context.send({ 
                    status: {
                        type: "connected",
                        url: connectionInfo.url,
                        timestamp: Date.now()
                    }
                });
                
                // Start ping timer
                this.startPing(context);
            };

            this.socket.onmessage = (event) => {
                try {
                    const data = this.parseMessage(event.data);
                    context.send({
                        message: {
                            data: data,
                            timestamp: Date.now(),
                            size: event.data.length
                        }
                    });
                } catch (error) {
                    context.send({
                        error: {
                            message: "Failed to parse incoming message",
                            data: event.data,
                            error: error.message
                        }
                    });
                }
            };

            this.socket.onclose = (event) => {
                this.stopPing();
                
                const closeInfo = {
                    type: "disconnected",
                    code: event.code,
                    reason: event.reason,
                    wasClean: event.wasClean,
                    timestamp: Date.now()
                };

                context.send({ status: closeInfo });

                // Attempt reconnection if not a clean close
                if (!event.wasClean && this.reconnectCount < this.config.reconnectAttempts) {
                    this.attemptReconnect(connectionInfo, context);
                }
            };

            this.socket.onerror = (error) => {
                context.send({
                    error: {
                        message: "WebSocket error",
                        timestamp: Date.now()
                    }
                });
            };

        } catch (error) {
            context.send({
                error: {
                    message: "Failed to create WebSocket connection",
                    error: error.message,
                    url: connectionInfo.url
                }
            });
        }
    }

    attemptReconnect(connectionInfo, context) {
        this.reconnectCount++;
        const delay = this.config.reconnectDelay * Math.pow(2, this.reconnectCount - 1);

        context.send({
            status: {
                type: "reconnecting",
                attempt: this.reconnectCount,
                maxAttempts: this.config.reconnectAttempts,
                delay: delay
            }
        });

        setTimeout(() => {
            this.connect(connectionInfo, context);
        }, delay);
    }

    disconnect(context) {
        if (this.socket) {
            this.stopPing();
            this.socket.close(1000, "Client disconnect");
            this.socket = null;
        }

        context.send({
            status: {
                type: "disconnected",
                reason: "Client initiated",
                timestamp: Date.now()
            }
        });
    }

    sendMessage(messageData, context) {
        if (!this.socket || this.socket.readyState !== WebSocket.OPEN) {
            context.send({
                error: {
                    message: "WebSocket not connected",
                    messageData: messageData
                }
            });
            return;
        }

        try {
            const message = this.formatMessage(messageData);
            
            if (message.length > this.config.maxMessageSize) {
                context.send({
                    error: {
                        message: "Message too large",
                        size: message.length,
                        maxSize: this.config.maxMessageSize
                    }
                });
                return;
            }

            this.socket.send(message);
            
            context.send({
                status: {
                    type: "message_sent",
                    size: message.length,
                    timestamp: Date.now()
                }
            });

        } catch (error) {
            context.send({
                error: {
                    message: "Failed to send message",
                    error: error.message,
                    messageData: messageData
                }
            });
        }
    }

    parseMessage(data) {
        // Try to parse as JSON first
        try {
            return JSON.parse(data);
        } catch {
            return data; // Return as string if not JSON
        }
    }

    formatMessage(data) {
        if (typeof data === 'string') {
            return data;
        }
        return JSON.stringify(data);
    }

    startPing(context) {
        this.stopPing();
        
        this.pingTimer = setInterval(() => {
            if (this.socket && this.socket.readyState === WebSocket.OPEN) {
                this.socket.send(JSON.stringify({ type: 'ping', timestamp: Date.now() }));
            }
        }, this.config.pingInterval);
    }

    stopPing() {
        if (this.pingTimer) {
            clearInterval(this.pingTimer);
            this.pingTimer = null;
        }
    }
}
```

## 5. Interactive UI Actor

Finally, let's create an actor that can interact with the DOM and handle user interactions.

```javascript
class UIInteractionActor {
    constructor() {
        this.inports = ["createElement", "updateElement", "removeElement", "addEventListener"];
        this.outports = ["element", "event", "error"];
        
        this.config = {
            containerSelector: "#app",
            eventTypes: ["click", "input", "change", "submit"]
        };
        
        this.elements = new Map(); // Track created elements
        this.listeners = new Map(); // Track event listeners
    }

    run(context) {
        if (context.input.createElement) {
            this.createElement(context.input.createElement, context);
        }

        if (context.input.updateElement) {
            this.updateElement(context.input.updateElement, context);
        }

        if (context.input.removeElement) {
            this.removeElement(context.input.removeElement, context);
        }

        if (context.input.addEventListener) {
            this.addEventListener(context.input.addEventListener, context);
        }
    }

    createElement(elementData, context) {
        try {
            const element = document.createElement(elementData.tag || 'div');
            
            // Set attributes
            if (elementData.attributes) {
                for (const [key, value] of Object.entries(elementData.attributes)) {
                    element.setAttribute(key, value);
                }
            }

            // Set properties
            if (elementData.properties) {
                for (const [key, value] of Object.entries(elementData.properties)) {
                    element[key] = value;
                }
            }

            // Set styles
            if (elementData.styles) {
                for (const [key, value] of Object.entries(elementData.styles)) {
                    element.style[key] = value;
                }
            }

            // Set content
            if (elementData.textContent) {
                element.textContent = elementData.textContent;
            }
            
            if (elementData.innerHTML) {
                element.innerHTML = elementData.innerHTML;
            }

            // Add to container
            const container = document.querySelector(this.config.containerSelector);
            if (container) {
                container.appendChild(element);
            }

            // Generate unique ID if not provided
            const elementId = elementData.id || `element_${Date.now()}_${Math.random().toString(36).substr(2, 9)}`;
            if (!element.id) {
                element.id = elementId;
            }

            // Store element reference
            this.elements.set(elementId, element);

            context.send({
                element: {
                    id: elementId,
                    tag: element.tagName.toLowerCase(),
                    created: true,
                    timestamp: Date.now()
                }
            });

        } catch (error) {
            context.send({
                error: {
                    message: "Failed to create element",
                    error: error.message,
                    elementData: elementData
                }
            });
        }
    }

    updateElement(updateData, context) {
        try {
            const element = this.elements.get(updateData.id) || 
                           document.getElementById(updateData.id);
            
            if (!element) {
                context.send({
                    error: {
                        message: "Element not found",
                        id: updateData.id
                    }
                });
                return;
            }

            // Update attributes
            if (updateData.attributes) {
                for (const [key, value] of Object.entries(updateData.attributes)) {
                    if (value === null) {
                        element.removeAttribute(key);
                    } else {
                        element.setAttribute(key, value);
                    }
                }
            }

            // Update properties
            if (updateData.properties) {
                for (const [key, value] of Object.entries(updateData.properties)) {
                    element[key] = value;
                }
            }

            // Update styles
            if (updateData.styles) {
                for (const [key, value] of Object.entries(updateData.styles)) {
                    element.style[key] = value;
                }
            }

            // Update content
            if (updateData.textContent !== undefined) {
                element.textContent = updateData.textContent;
            }
            
            if (updateData.innerHTML !== undefined) {
                element.innerHTML = updateData.innerHTML;
            }

            context.send({
                element: {
                    id: updateData.id,
                    updated: true,
                    timestamp: Date.now()
                }
            });

        } catch (error) {
            context.send({
                error: {
                    message: "Failed to update element",
                    error: error.message,
                    updateData: updateData
                }
            });
        }
    }

    removeElement(removeData, context) {
        try {
            const element = this.elements.get(removeData.id) || 
                           document.getElementById(removeData.id);
            
            if (!element) {
                context.send({
                    error: {
                        message: "Element not found",
                        id: removeData.id
                    }
                });
                return;
            }

            // Remove event listeners
            const listeners = this.listeners.get(removeData.id);
            if (listeners) {
                for (const listener of listeners) {
                    element.removeEventListener(listener.type, listener.handler);
                }
                this.listeners.delete(removeData.id);
            }

            // Remove from DOM
            element.remove();

            // Remove from tracking
            this.elements.delete(removeData.id);

            context.send({
                element: {
                    id: removeData.id,
                    removed: true,
                    timestamp: Date.now()
                }
            });

        } catch (error) {
            context.send({
                error: {
                    message: "Failed to remove element",
                    error: error.message,
                    removeData: removeData
                }
            });
        }
    }

    addEventListener(listenerData, context) {
        try {
            const element = this.elements.get(listenerData.elementId) || 
                           document.getElementById(listenerData.elementId);
            
            if (!element) {
                context.send({
                    error: {
                        message: "Element not found",
                        id: listenerData.elementId
                    }
                });
                return;
            }

            const handler = (event) => {
                const eventData = {
                    type: event.type,
                    elementId: listenerData.elementId,
                    timestamp: Date.now(),
                    target: {
                        id: event.target.id,
                        tagName: event.target.tagName,
                        value: event.target.value,
                        checked: event.target.checked
                    }
                };

                // Add specific event data
                if (event.type === 'click') {
                    eventData.coordinates = {
                        clientX: event.clientX,
                        clientY: event.clientY,
                        pageX: event.pageX,
                        pageY: event.pageY
                    };
                }

                if (event.type === 'input' || event.type === 'change') {
                    eventData.value = event.target.value;
                }

                context.send({ event: eventData });
            };

            element.addEventListener(listenerData.eventType, handler);

            // Track the listener
            if (!this.listeners.has(listenerData.elementId)) {
                this.listeners.set(listenerData.elementId, []);
            }
            this.listeners.get(listenerData.elementId).push({
                type: listenerData.eventType,
                handler: handler
            });

            context.send({
                element: {
                    id: listenerData.elementId,
                    eventType: listenerData.eventType,
                    listenerAdded: true,
                    timestamp: Date.now()
                }
            });

        } catch (error) {
            context.send({
                error: {
                    message: "Failed to add event listener",
                    error: error.message,
                    listenerData: listenerData
                }
            });
        }
    }
}
```

## Complete Working Example

Let's build a complete application that uses all the actors we've created:

```javascript
// Complete Browser Actor Demo Application
class BrowserDemo {
    constructor() {
        this.graph = null;
        this.network = null;
        this.isRunning = false;
    }

    async initialize() {
        // Initialize Browser
        await init();
        init_panic_hook();

        // Create demo graph
        this.createDemoGraph();
        
        // Register all actors
        this.registerActors();
        
        // Setup UI
        this.setupUI();
        
        console.log("✅ Browser Actor Demo initialized");
    }

    createDemoGraph() {
        this.graph = new Graph("BrowserActorDemo", true, {
            description: "Comprehensive Browser actor demonstration",
            version: "1.0.0"
        });

        // Add actors
        this.graph.addNode("dataTransform", "DataTransformActor", {
            x: 100, y: 100,
            description: "Data transformation processor"
        });

        this.graph.addNode("fileProcessor", "FileProcessorActor", {
            x: 300, y: 100,
            description: "File processing handler"
        });

        this.graph.addNode("apiClient", "WebAPIClientActor", {
            x: 500, y: 100,
            description: "Web API client"
        });

        this.graph.addNode("websocket", "WebSocketActor", {
            x: 100, y: 300,
            description: "WebSocket connection"
        });

        this.graph.addNode("uiManager", "UIInteractionActor", {
            x: 300, y: 300,
            description: "UI interaction manager"
        });

        // Create connections
        this.graph.addConnection("dataTransform", "result", "fileProcessor", "operation");
        this.graph.addConnection("fileProcessor", "content", "apiClient", "request");
        this.graph.addConnection("apiClient", "response", "websocket", "send");
        this.graph.addConnection("websocket", "message", "uiManager", "createElement");

        // Add some initial data
        this.graph.addInitial([1, 2, 3, 4, 5], "dataTransform", "data");
    }

    registerActors() {
        this.network = new GraphNetwork(this.graph);
        
        this.network.registerActor("DataTransformActor", new DataTransformActor());
        this.network.registerActor("FileProcessorActor", new FileProcessorActor());
        this.network.registerActor("WebAPIClientActor", new WebAPIClientActor());
        this.network.registerActor("WebSocketActor", new WebSocketActor());
        this.network.registerActor("UIInteractionActor", new UIInteractionActor());

        // Monitor network events
        this.network.next((event) => {
            this.handleNetworkEvent(event);
        });
    }

    setupUI() {
        document.body.innerHTML = `
            <div id="demo-app">
                <h1>Browser Actor Development Demo</h1>
                
                <div class="controls">
                    <button id="startNetwork">Start Network</button>
                    <button id="stopNetwork">Stop Network</button>
                    <button id="testDataTransform">Test Data Transform</button>
                    <button id="testFileUpload">Test File Upload</button>
                    <button id="testAPI">Test API Call</button>
                    <button id="testWebSocket">Test WebSocket</button>
                </div>
                
                <div class="output">
                    <h3>Network Events:</h3>
                    <div id="events" style="height: 200px; overflow-y: auto; border: 1px solid #ccc; padding: 10px;"></div>
                </div>
                
                <div class="file-upload">
                    <h3>File Upload Test:</h3>
                    <input type="file" id="fileInput" />
                    <select id="fileOperation">
                        <option value="read">Read</option>
                        <option value="parse">Parse</option>
                        <option value="analyze">Analyze</option>
                    </select>
                </div>
                
                <div id="dynamic-content">
                    <h3>Dynamic UI Elements:</h3>
                    <!-- UI actor will add elements here -->
                </div>
            </div>
        `;

        // Add event listeners
        document.getElementById('startNetwork').onclick = () => this.startNetwork();
        document.getElementById('stopNetwork').onclick = () => this.stopNetwork();
        document.getElementById('testDataTransform').onclick = () => this.testDataTransform();
        document.getElementById('testFileUpload').onclick = () => this.testFileUpload();
        document.getElementById('testAPI').onclick = () => this.testAPI();
        document.getElementById('testWebSocket').onclick = () => this.testWebSocket();
    }

    async startNetwork() {
        if (!this.isRunning) {
            await this.network.start();
            this.isRunning = true;
            this.logEvent("Network started");
        }
    }

    stopNetwork() {
        if (this.isRunning) {
            this.network.shutdown();
            this.isRunning = false;
            this.logEvent("Network stopped");
        }
    }

    async testDataTransform() {
        const testData = [
            Math.random() * 100,
            Math.random() * 100,
            "  Test String  ",
            { test_field: "value", another_field: 123 }
        ];

        const result = await this.network.executeActor("dataTransform", {
            data: testData,
            config: { operation: "normalize", precision: 2 }
        });

        this.logEvent("Data transform result", result);
    }

    testFileUpload() {
        const fileInput = document.getElementById('fileInput');
        const operation = document.getElementById('fileOperation').value;
        
        if (fileInput.files.length > 0) {
            const file = fileInput.files[0];
            this.network.executeActor("fileProcessor", {
                file: file,
                operation: { type: operation, format: 'auto' }
            });
        } else {
            alert("Please select a file first");
        }
    }

    async testAPI() {
        // Test with a public API
        const result = await this.network.executeActor("apiClient", {
            request: {
                url: "https://jsonplaceholder.typicode.com/posts/1",
                method: "GET",
                responseType: "json"
            }
        });

        this.logEvent("API call result", result);
    }

    testWebSocket() {
        // Test WebSocket connection (you'll need a WebSocket server)
        this.network.executeActor("websocket", {
            connect: {
                url: "wss://echo.websocket.org/",
                protocols: []
            }
        });
    }

    handleNetworkEvent(event) {
        switch (event._type) {
            case "FlowTrace":
                this.logEvent(`Data flow: ${event.from.actorId}:${event.from.port} → ${event.to.actorId}:${event.to.port}`, event.from.data);
                break;
                
            case "ActorStarted":
                this.logEvent(`Actor started: ${event.actorId}`);
                break;
                
            case "ActorStopped":
                this.logEvent(`Actor stopped: ${event.actorId}`);
                break;
                
            case "ProcessError":
                this.logEvent(`Error in ${event.actorId}: ${event.error}`, null, "error");
                break;
                
            default:
                this.logEvent(`Network event: ${event._type}`, event);
        }
    }

    logEvent(message, data = null, type = "info") {
        const eventsDiv = document.getElementById('events');
        const timestamp = new Date().toLocaleTimeString();
        
        const eventElement = document.createElement('div');
        eventElement.className = `event event-${type}`;
        eventElement.style.marginBottom = '5px';
        eventElement.style.padding = '5px';
        eventElement.style.backgroundColor = type === 'error' ? '#ffe6e6' : '#e6f3ff';
        
        let content = `[${timestamp}] ${message}`;
        if (data) {
            content += `\nData: ${JSON.stringify(data, null, 2)}`;
        }
        
        eventElement.textContent = content;
        eventsDiv.appendChild(eventElement);
        eventsDiv.scrollTop = eventsDiv.scrollHeight;
    }
}

// Initialize the demo when the page loads
document.addEventListener('DOMContentLoaded', async () => {
    const demo = new BrowserActorDemo();
    await demo.initialize();
});
```

## Best Practices and Performance Tips

### 1. State Management
```javascript
// ✅ Good: Batch state operations
class EfficientActor {
    run(context) {
        const state = context.state.getAll();
        
        // Modify locally
        state.counter = (state.counter || 0) + 1;
        state.lastUpdate = Date.now();
        state.processedItems = (state.processedItems || []);
        state.processedItems.push(context.input.data);
        
        // Write once
        context.state.setAll(state);
    }
}

// ❌ Avoid: Multiple state operations
class InefficientActor {
    run(context) {
        const counter = context.state.get('counter') || 0;
        context.state.set('counter', counter + 1);
        context.state.set('lastUpdate', Date.now());
        
        const items = context.state.get('processedItems') || [];
        items.push(context.input.data);
        context.state.set('processedItems', items);
    }
}
```

### 2. Error Handling
```javascript
class RobustActor {
    run(context) {
        try {
            this.processInput(context);
        } catch (error) {
            this.handleError(error, context);
        }
    }
    
    handleError(error, context) {
        // Log error details
        console.error(`${this.constructor.name} error:`, error);
        
        // Send structured error information
        context.send({
            error: {
                message: error.message,
                stack: error.stack,
                input: context.input,
                timestamp: Date.now(),
                actorType: this.constructor.name
            }
        });
        
        // Update error statistics
        const stats = context.state.get('errorStats') || { count: 0, lastError: null };
        stats.count++;
        stats.lastError = Date.now();
        context.state.set('errorStats', stats);
    }
}
```

### 3. Memory Management
```javascript
class MemoryAwareActor {
    constructor() {
        this.inports = ["input"];
        this.outports = ["output"];
        this.config = { maxCacheSize: 1000 };
    }
    
    run(context) {
        // Clean up old cache entries
        this.cleanupCache(context);
        
        // Process input
        const result = this.processData(context.input.input);
        
        // Cache result if within limits
        this.cacheResult(result, context);
        
        context.send({ output: result });
    }
    
    cleanupCache(context) {
        const cache = context.state.get('cache') || {};
        const entries = Object.entries(cache);
        
        if (entries.length > this.config.maxCacheSize) {
            // Sort by timestamp and keep only recent entries
            const sorted = entries.sort((a, b) => b[1].timestamp - a[1].timestamp);
            const cleaned = Object.fromEntries(sorted.slice(0, this.config.maxCacheSize));
            context.state.set('cache', cleaned);
        }
    }
}
```

## Conclusion

This tutorial covered the essential patterns for developing Browser actors in browser environments:

1. **Data Transformation** - Processing and manipulating data with stateful operations
2. **Web API Integration** - Making HTTP requests with retry logic and error handling
3. **File Processing** - Handling browser file operations with progress tracking
4. **Real-time Communication** - WebSocket connections with automatic reconnection
5. **UI Interaction** - DOM manipulation and event handling

### Key Takeaways

- **State Management**: Use batch operations for better performance
- **Error Handling**: Implement comprehensive error reporting and recovery
- **Async Operations**: Handle promises and timeouts properly
- **Memory Management**: Clean up resources and limit cache sizes
- **Browser APIs**: Leverage native browser capabilities effectively

### Next Steps

- **[Complete Browser API Reference](../api/wasm/getting-started.md)** - Full API documentation
- **[Browser Actors Guide](../api/wasm/actors-in-browser.md)** - Detailed actor patterns
- **[Browser Workflow Editor](browser-workflow-editor.md)** - Building visual editors
- **[Performance Optimization](performance-optimization.md)** - Advanced optimization techniques

The examples in this tutorial provide a solid foundation for building sophisticated browser-based workflow applications using Reflow's Browser bindings.
