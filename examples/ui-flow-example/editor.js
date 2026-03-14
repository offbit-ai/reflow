import './controls.js';
import './node.js';
import { Node } from './node.js';
import { Port, PortViewTrait } from './port.js';
import { Connection } from './connection.js';
import { Controls } from './controls.js';
import { Group } from './group.js';
import { getGraphWorker, Namespace } from "./util.js";
import { ContextMenu } from './context-menu.js';
import { EditorEvents } from './events.js';
import { EditorState } from './store.js';
import Icons from './ui/icons.js';



export class Editor extends HTMLElement {
    constructor() {
        super();
        this.lastPanPoint = { x: 0, y: 0 };
        this.offset = { x: 0, y: 0 };
        this.worldMousePos = { x: 0, y: 0 };
        this.zoom = 0.68;
        this._nodes = [];
        this._groups = [];
        this.zoomSensitivity = 0.001;
        /**
         * @type {Array<Connection>}
         */
        this._connections = [];

        this.selectionRect = null;
        this.selectionStart = null;

        this.worker = getGraphWorker();

        this.renderRoot = this.attachShadow({ mode: 'open' });
        this.renderRoot.innerHTML = `
            <div class="editor">
                ${this._background}
                <div class="nodes-container">
                
                </div>
                ${this._connectionLine}
                
                <${Namespace}-controls></${Namespace}-controls>
                <div class="component_picker">
                    <div class="component_picker_container">
                        <div class="component_search">
                            <input placeholder="Search components by keywords" />
                        </div>
                        <div class="components"></div>
                    </div>
                    <div class="toggle">
                    </div>
                </div>
            </div>
        `;
        const style = document.createElement('style');
        style.textContent = Editor.styles;
        this.renderRoot.appendChild(style);

        this.renderRoot.querySelector(".component_picker > .toggle").appendChild(Icons.nodesIcon)


        this.setup();

        this._updateBg();
    }

    static get styles() {
        return `
            .editor {
                position: relative;
                width: 100%;
                height: 100%;
                border: 1px solid black;
                overflow: hidden;
                user-select: none;
                background:#22222288;
            }
            .component_picker {
                position:absolute;
                left: -300px;
                top: 0;
                width: 300px;
                height: 100%;
                transition:left 0.5s;
            }
            .component_picker > .toggle {
                position: absolute;
                background:#222222;
                top: 10px;
                left: 300px;
                width: 28px;
                height:32px;
                float:right;
                border-top-right-radius: 6px;
                border-bottom-right-radius: 6px;
                cursor: pointer;
                display: flex;
                align-items: center;
                justify-content: center;
            }
            .component_picker > .toggle > div {
                height: 20px;
            }
            .component_picker > .toggle > div > svg > path {
                fill: #fafafa88;
            }

            .component_picker_container{
                display: flex;
                width: 100%;
                height: 100%;
                background:#22222288;
                border-right: 5px solid #222222;
                flex-grow:1;
                flex-shrink:1;
                flex-direction: column;
            }

            .component_picker_container > .components {
                display: flex;
                width: 100%;
                height: 100%;
                overflow-x: hidden;
                overflow-y: auto;
            }

            .component_search {
                padding: 10px;
                height: 35px;
                background: #22222288;
                border-bottom: 2px solid #111;
                display: flex;
                justify-content: center;
                align-items: center;
            }
            .component_search > input {
                background: #22222288;
                border: 1px solid #fafafa88;
                outline: 0;
                color: #fff;
                height: 20px;
                width: 95%;
                border-radius: 8px;
                padding: 5px;
            }
           
            .connection-line {
                position: absolute;
                top: 0;
                left: 0;
                width: 100%;
                height: 100%;
                pointer-events: none;
                /**background: rgba(13, 51, 250, 0.4)**/
            }
            .connection-line > path {
                pointer-events: stroke;
            }
            .connection-line > path:hover {
                stroke: fuchsia;
                cursor: pointer;
            }
            .nodes-container{
                position: absolute;
                top: 0;
                left: 0;
                width: 100%;
                height: 100%;
               /** background: rgba(216, 1, 3, 0.4) **/
            }
            .group{
                width: fit-content;
                height: fit-content;
                display: block;
                position: absolute;
                top: 0;
                left: 0;
            }
            .selection-rect {
                pointer-events: none;
                z-index: 1000;
            }
        `
    }
    connectedCallback() {

    }

    get _connectionLine() {
        return `
            <svg class="connection-line" xmlns="http://www.w3.org/2000/svg">
            </svg>
        `;
    }

    get _background() {
        return `
                <svg id="bg" viewbox="0,0,800,640" xmlns="http://www.w3.org/2000/svg">
                    <defs>
                        <pattern id="inner-grid" width="10" height="10" patternUnits="userSpaceOnUse">
                            <rect width="100%" height="100%" fill="none" stroke="#111" stroke-width="0.5"></rect>
                        </pattern>
                        <pattern id="grid" width="100" height="100" patternUnits="userSpaceOnUse">
                            <rect width="100%" height="100%" fill="url(#inner-grid)" stroke="#11111188" stroke-width="3"></rect>
                        </pattern>
                    </defs>
                    <rect x="0" y="0" width="100%" height="100%" fill="url(#grid)"></rect>
                </svg>
        `
    }



    get background() {
        return this.renderRoot.querySelector('#bg');
    }



    /**
     * @type {HTMLDivElement}
     */
    get container() {
        return this.renderRoot.querySelector('.editor');
    }

    get wrapper() {
        return this.renderRoot.querySelector('.wrapper');
    }

    get nodesContainer() {
        return this.renderRoot.querySelector('.nodes-container');
    }

    get connectionLine() {
        return this.renderRoot.querySelector('.connection-line');
    }


    /**
     * Component Picker
     *
     * @readonly
     * @type {HTMLDivElement}
     */
    get componentsPicker() {
        return this.renderRoot.querySelector('.component_picker');
    }

    /**
     * Component Picker Toggle
     *
     * @readonly
     * @type {HTMLDivElement}
     */
    get componentsPickerToggle() {
        return this.renderRoot.querySelector('.component_picker > .toggle');
    }

    /**
     * @type {Controls}
     */
    get controls() {
        return this.renderRoot.querySelector(`${Namespace}-controls`);
    }

    /**
     * @type {Array<string>}
     */
    get nodes() {
        return this._nodes;
    }

    set nodes(_nodes) {
        this._nodes = _nodes;
    }

    /**
     * @type {Array<string>}
     */
    get groups() {
        return this._groups;
    }

    set groups(_groups) {
        this._groups = _groups;
    }

    /**
     * @type {Array<Connection>}
     */
    get connections() {
        return this._connections;
    }

    set connections(connections) {
        this._connections = connections;
    }

    setup() {
        this.worker.addEventListener("message", ((ev) => {

            if (ev.data === EditorEvents.READY) {
                this.worker.postMessage({ event: EditorEvents.INIT, name: "My Graph" });
                return;
            }
            if (ev.data.type === EditorEvents.GRAPH_LOADED) {

                /**
                 * @type {import('../../crates/reflow_network/pkg/reflow_network.js').GraphExport}
                 */
                let graph_json = ev.data.graph;

                Array.from(graph_json.processes.values()).sort((a, b) => b.metadata.get("index") > a.metadata.get("index")).forEach((node, index) => {
                    /**
                     * @typedef {Map} PortItem
                     * @property {string} id 
                     * @property {string} name 
                     * @property {string} trait
                     * @property {any} field
                     */
                    let metadata = Object.fromEntries(node.metadata);

                    /**
                     * @type {Array<PortItem>}
                     */
                    let _inports = metadata.inports;

                    let inports = _inports.map((p) => Object.fromEntries(p)).map((p) => {

                        let viewModel = p.value;
                        if (Array.isArray(p.value)) {
                            viewModel = p.value.map((f) => Object.fromEntries(f));
                        } else if (typeof p.value === "object") {
                            viewModel = Object.fromEntries(p.value)
                        } else {
                            viewModel = p.value;
                        }
                        return ({ nodeid: node.id, direction: "in", name: p.name, id: p.id, trait: p.trait, viewModel })
                    });

                    /**
                     * @type {Array<PortItem>}
                     */
                    let _outports = metadata.outports;
                    let outports = _outports.map((p) => Object.fromEntries(p)).map((p) => {
                        let viewModel = p.value;
                        if (Array.isArray(p.value)) {
                            viewModel = p.value.map((f) => Object.fromEntries(f));
                        } else if (typeof p.value === "object") {
                            viewModel = Object.fromEntries(p.value)
                        } else {
                            viewModel = p.value;
                        }
                        return ({ nodeid: node.id, direction: "out", name: p.name, id: p.id, trait: p.trait, viewModel })
                    });

                    let position = node.metadata.get("position");
                    let _node = new Node({ id: node.id, process: node.component, index, name: metadata.name, position: { x: position.get("x"), y: position.get("y") }, inports, outports });
                    this.addNode(_node, false);
                });
                graph_json.inports.forEach((port) => {
                    let node = this.getNode(port.node_id);
                    if (node.getPortById(port.port_id)) {
                        return;
                    }
                    let viewModel = port.value;
                    if (Array.isArray(port.value)) {
                        viewModel = port.value.map((f) => Object.fromEntries(f));
                    } else if (typeof port.value === "object") {
                        viewModel = Object.fromEntries(port.value)
                    } else {
                        viewModel = p.value;
                    }

                    let position = Object.fromEntries(port.metadata.get("position"));
                    let trait = Object.hasOwn(port.metadata, "trait") ? Object.fromEntries(port.metadata.get("trait")) : undefined;
                    let _port = new Port({ nodeid: node.id, direction: "out", name: port.port_name, id: port.port_id, trait, viewModel, position });

                    let newPorts = [...node.inports];

                    newPorts.push(_port);
                    node.inports = newPorts;
                });

                graph_json.outports.forEach((port) => {
                    let node = this.getNode(port.node_id);
                    if (node.getPortById(port.port_id)) {
                        return;
                    }
                    let viewModel = port.value;
                    if (Array.isArray(port.value)) {
                        viewModel = port.value.map((f) => Object.fromEntries(f));
                    } else if (typeof port.value === "object") {
                        viewModel = Object.fromEntries(port.value)
                    } else {
                        viewModel = p.value;
                    }

                    let position = Object.fromEntries(port.metadata.get("position"));
                    let trait = Object.hasOwn(port.metadata, "trait") ? Object.fromEntries(port.metadata.get("trait")) : undefined;
                    let _port = new Port({ nodeid: node.id, direction: "out", name: port.port_name, id: port.port_id, trait, viewModel, position });
                    let newPorts = [...node.outports];

                    newPorts.push(_port);
                    node.outports = newPorts;
                });
                graph_json.connections.forEach((connection) => {

                    let from = {
                        nodeId: connection.from.node_id,
                        port: connection.from.port_name,
                        portId: connection.from.port_id
                    };
                    let to = {
                        nodeId: connection.to.node_id,
                        port: connection.to.port_name,
                        portId: connection.to.port_id
                    }
                    this.addConnection(from, to, false);
                });


                graph_json.groups.forEach((group, index)=>{
                    const metadata = Object.fromEntries(group.metadata);
                    const nodes = group.nodes.map((node_id)=> {
                        const node = this.getNode(node_id);
                        node.dataset["group"] = group.id;
                        return node;
                    });
                    const _group = new Group(metadata.name, metadata.description, nodes, group.id);
                    
                    this.addGroup(_group, false);
                    
                    const position = Object.fromEntries(metadata.position);
                    _group.position = position;
                    _group.updateGroupTransform(this.zoom, this.offset, false);
                    _group.container.style.left = `${position.x}px`;
                    _group.container.style.top =`${position.y}px`;
                    
                    setTimeout(()=> this._updateConnections(), 0);
                    _group.registerUpdateEvents(this.worker);
                    
                })


                this.firstUpdated();
                return;
            }
        }).bind(this));
    }

    /**
     * Add Node
     * @param {import('./node.js').Node} node 
     * @param {Boolean} emit
     * @returns {string}
     */
    addNode(node, emit = true) {
        node.setAttribute("class", "node");

        this.nodesContainer.appendChild(node);
        this.nodes.push(node.id);
        const options = node.getMetadata();

        if (emit) {
            this.worker.postMessage({
                event: EditorEvents.ADD_NODE, node: {
                    id: node.id, process: node.process, metadata: {
                        index: this.nodes.length - 1,
                        ...options
                    }
                }
            });
        }
        return node.id;
    }

    /**
     * 
     * @param {Node} node 
     */
    updateNodeMetadata(node, emit = false) {
        /**
         * @type {import('./node.js').NodeOptions}
         */
        let options = node.getMetadata(); 
        if (emit) {
            this.worker.postMessage({
                event: EditorEvents.UPDATE_NODE_METADATA, node: {
                    id: node.id, process: options.process, metadata: {
                        index: this.nodes.indexOf(node.id),
                        ...options
                    }
                }
            });
        }
    }

    /**
     * Add group of nodes
     * @param {Group} group 
     * @param {Boolean} emit
     */
    addGroup(group, emit = true) {
        group.offset = this.offset;
        group.setAttribute("class", "group");
        group.setAttribute("id", group.id);

        this.nodesContainer.appendChild(group);

        this.groups.push(group.id);

        if (emit) {
            this.worker.postMessage({
                event: EditorEvents.ADD_GROUP, group: {
                    id: group.id,
                    nodes: [...new Set(group.nodeIds)],
                    metadata: {
                        position: group.position,
                        name: group.name,
                        description: group.description
                    }
                }
            });
            group.registerUpdateEvents(this.worker);
        }

        return { groupId: group.id, nodes: group.nodeIds };
    }

    /**
     * 
     * @param {string} id 
     * @returns {Node}
     */
    getNode(id) {
        /**
         * @type {NodeList<Node>}
         */
        let _nodeRoots = this.nodesContainer.querySelectorAll(`${Namespace}-node`);
        /**
        * @type {Group}
        */
        const groups = this.nodesContainer.querySelectorAll(".group");
        const nodes = Array.from(groups).reduce((prev, next) => {
            return [...prev, next.getNode(id)]
        }, []).filter((v) => v !== undefined);

        if (nodes.length > 0) {
            return nodes[0]
        }
        /**
         * @type {Node}
         */
        const node = Array.from(_nodeRoots).find((node) => node.id === id);
        return node
    }

    /**
   * 
   * @param {string} name 
   * @returns {Node}
   */
    getNodeByName(name) {
        /**
         * @type {NodeList<Node>}
         */
        const _nodeRoots = this.nodesContainer.querySelectorAll(`${Namespace}-node`);
        /**
         * @type {Node}
         */
        const node = _nodeRoots.values().find((node) => node.name === name);
        return node
    }

    /**
     * Add Connection
     * @param {{nodeId:string, port:string, portId:string}} from 
     * @param {{nodeId:string, port:string, portId:string}} to 
     * @param {Boolean} emit
     */
    addConnection(from, to, emit = true) {
        const fromNode = this.getNode(from.nodeId);
        const fromPort = from.port ? fromNode.getPort(from.port) : fromNode.getPortById(from.portId);
        const toNode = this.getNode(to.nodeId);
        const toPort = to.port ? toNode.getPort(to.port) : toNode.getPortById(to.portId);

        const startPort = Array.from(fromNode.outportsContainer.querySelectorAll(`${Namespace}-port`)).find((port) => port?.id === fromPort?.id);
        const endPort = Array.from(toNode.inportsContainer.querySelectorAll(`${Namespace}-port`)).find((port) => port?.id === toPort?.id);

        if (startPort && endPort) {
            this._createConnection(from, startPort.portStub, to, endPort.portStub, emit);
        }
    }

    /**
     * @typedef {object} InitialPacket
     * @property {{nodeId:string, portId:string, port:string}}  to 
     * @property {any} data 
     */

    /**
     * Add an Initial packet
     * @param {InitialPacket} packet 
     * @param {Boolean} emit
     */
    addInitialPacket(packet, emit = true) {
        const toNode = this.getNode(packet.to.nodeId);
        const toPort = toNode.getPort(packet.to.port);
        /**
         * @type {PortViewTrait}
         */
        const trait = toPort.trait;

        switch (trait) {
            case PortViewTrait.OPTION: {
                toPort.setSelectedOption(packet.data);
                break;
            }
            case PortViewTrait.TEXT_INPUT:
            case PortViewTrait.NUMBER_INPUT:
            case PortViewTrait.BOOLEAN: {
                toPort.setData(packet.data);
                break;
            }
            default: { }
        }
        if (emit) {
            this.worker.postMessage({ event: EditorEvents.ADD_IIP, iip: { ...packet, trait: trait, position: toPort.position } });
        }
    }

    captureGraphEvents() {
        this.worker.addEventListener("message", ((ev) => {
            if (Object.hasOwn(ev.data, "_type")) {
                // console.log(ev.data);
                // switch (ev.data._type) {
                //     case "AddNode": this.addNode(new Node({
                //         id: ev.data.id,
                //         name: ev.data.component,

                //     }))
                // }
            }
        }).bind(this))
    }

    firstUpdated() {
        this.captureGraphEvents();

        this._updateConnections();
        this._updateTransform();
        this._updateBg();
        this._attachEvents();

    }

    _updateBg() {
        const bg = this.background;
        bg.style.position = "absolute";
        bg.style.top = "0";
        bg.style.left = "0";
        bg.setAttribute("viewBox", [0, 0, this.container.clientWidth, this.container.clientHeight])
        bg.style.width = `${this.container.clientWidth}px`;
        bg.style.height = `${this.container.clientHeight}px`;
        bg.style.pointerEvents = "none";
    }

    _attachEvents() {
        this.container.addEventListener('mousedown', this._mouseDown.bind(this));
        this.container.addEventListener('mousemove', this._mouseMove.bind(this));
        document.addEventListener('mousemove', this.updateWorldMousePos.bind(this));
        this.container.addEventListener('mouseup', this._mouseUp.bind(this));

        this.container.addEventListener('wheel', this._wheel.bind(this), { passive: false });

        this.container.addEventListener('contextmenu', this.contextMenuEventCb.bind(this));

        this.componentPickerOpen = false;

        this.componentsPickerToggle.addEventListener("mousedown", (e) => {
            if (!this.componentPickerOpen) {
                this.componentsPicker.style.left = "0px";
                this.componentPickerOpen = true;
            } else {
                this.componentsPicker.style.left = "-300px";
                this.componentPickerOpen = false;
            }
        });

        setTimeout(() => {
            this.controls.onZoomIn(this._zoomIn.bind(this));
            this.controls.onZoomOut(this._zoomOut.bind(this));
        }, 0);

        EditorState.subscribe((state) => {
            if (state.drawerResized && this.zoom != 1.0) {
                this.connectionLine.style.display = "none";
                setTimeout(() => {
                    this.connectionLine.style.display = "block";
                    this._updateBg();
                    this._updateTransform();
                    this._updateConnections();
                }, 450);
            }
        });
        window.addEventListener("resize", this._resize.bind(this));

    }

    _zoomIn() {
        this.zoom += 1 * 0.1;
        const newZoom = Math.max(0.5, Math.min(this.zoom, 2)); // Limit zoom between 0.1 and 5
        this.zoom = newZoom + this.zoomSensitivity;
        this._updateTransform();
        this._updateConnections();
    }
    _zoomOut() {
        this.zoom -= 1 * 0.1;
        const newZoom = Math.max(0.5, Math.min(this.zoom, 2)); // Limit zoom between 0.1 and 5
        this.zoom = newZoom;
        this._updateTransform();
        this._updateConnections();
    }

    updateWorldMousePos(e) {
        const rect = this.container.getBoundingClientRect();
        this.worldMousePos.x = (e.clientX - rect.left) / this.zoom + this.offset.x;
        this.worldMousePos.y = (e.clientY - rect.top) / this.zoom + this.offset.y;

        this.renderRoot.querySelectorAll(`${Namespace}-group`).forEach((group) => {
            group.updateWorldMousePos(this.worldMousePos.x, this.worldMousePos.y);
        });
    }

    _resize() {
        this._updateBg();
        this._updateTransform();
        this._updateConnections();
    }

    /**
     * 
     * @param {MouseEvent} e 
     * @returns 
     */
    _mouseDown(e) {
        this.updateWorldMousePos(e);
        this.selectionRect = null;
        this.selectionStart = null;
        const composedTarget = e.composedPath ? e.composedPath()[0] : e.target.closest('.node');

        if (composedTarget.classList.contains('port-stub')) {
            /**
             * @type {HTMLDivElement}
             */
            const port = composedTarget;
            if (port.dataset["direction"] === "out") {
                this.draggingNode = null;
                this.draggingPort = port;
                this.tempConnection = new Connection(this.draggingPort, this.worldMousePos, this.worldMousePos, this.connectionLine, this.zoom, this.offset);
                e.preventDefault();
                return;
            }
        }
        if (e.target.closest('.node')) {
            /**
             * @type {Node}
             */
            this.draggingNode = e.target.closest('.node');
            this.draggingNode.startDrag(this.worldMousePos.x, this.worldMousePos.y);
            // e.preventDefault();
            return;
        }
        if (composedTarget.classList.contains('node-container')) {
            /**
             * @type {Node}
             */
            this.draggingNode = composedTarget.parentNode.host;
            this.draggingNode.startDrag(this.worldMousePos.x, this.worldMousePos.y);
            e.preventDefault();
            return;
        }
        if (e.target.closest(`${Namespace}-group`)) {
            /**
             * @type {Group}
             */
            this.draggingGroup = e.target.closest(`${Namespace}-group`);
            this.draggingGroup.startDrag(this.worldMousePos.x, this.worldMousePos.y, this.offset);
            // e.preventDefault();
            return;
        }

        if (e.button === 0 && composedTarget.classList.contains('nodes-container') && this.zoom <= 1) {
            this._startSelection(e.clientX, e.clientY);
            e.preventDefault();
            return;
        }
    }

    /**
     * 
     * @param {MouseEvent} e 
     * @returns 
     */
    _mouseMove(e) {

        if (this.draggingNode) {
            this.draggingNode.drag(this.worldMousePos.x, this.worldMousePos.y);
            this._updateConnections();
            this._updateGroups();
            const node = this.draggingNode;
            this.updateNodeMetadata(node, true);
            return;
        }
        if (this.draggingPort && this.tempConnection && this.draggingPort.dataset["direction"] === "out") {
            this.tempConnection.startDrag = true;
            this.tempConnection.updateEnd(e.clientX, e.clientY);
            return;
        }
        if (this.draggingGroup) {
            this.draggingGroup.drag(this.worldMousePos.x, this.worldMousePos.y, this.offset);
            this._updateConnections();
            return;
        }
        if (this.selectionStart) {
            this._updateSelection(e.clientX, e.clientY);
            return;
        }

    }

    _mouseUp(e) {
        if (this.draggingNode) {
            this.draggingNode = null;
        }

        if (this.draggingPort) {
            const composedTarget = e.composedPath ? e.composedPath()[0] : e.target.closest('.node');
            if (composedTarget.classList.contains('port-stub')) {
                /**
                * @type {HTMLDivElement}
                */
                const portUnderMouse = composedTarget;

                // Check if connection is valid (e.g., outport to inport)
                if (portUnderMouse && portUnderMouse.id !== this.draggingPort.id && this.draggingPort.dataset["direction"] === "out" && portUnderMouse.dataset['direction'] === "in") {
                    this._createConnection({ nodeId: this.draggingPort.dataset['nodeId'], port: this.draggingPort.dataset['name'], portId: this.draggingPort.id }, this.draggingPort, { nodeId: portUnderMouse.dataset['nodeId'], port: portUnderMouse.dataset['name'], portId: portUnderMouse.id }, portUnderMouse);
                }
            }
            if (this.tempConnection) {
                this.tempConnection.startDrag = false;
                this.connectionLine.removeChild(this.tempConnection.element)
                this.tempConnection = null
            }
        }
        if (this.draggingGroup) {
            this.draggingGroup = null;
        }

        if (this.selectionRect) {
            this._endSelection();
        }
        if (this.container.contains(this.contextMenu) && !e.target.closest(`${Namespace}-context-menu`)) {
            this.container.removeChild(this.contextMenu);
        }

        if (this.resizing) {
            this.resizeStartY = undefined;
            this.resizing = false;
        }

    }

    _wheel(e) {
        if (!e.target.closest('.component_picker')) {
            e.preventDefault();
            const CMDKey = navigator.userAgent.includes("Mac") ? e.metaKey : e.ctrlKey;
            if (CMDKey) {

                // Calculate new zoom
                const zoomFactor = 1 - e.deltaY * this.zoomSensitivity;
                const newZoom = Math.max(0.5, Math.min(this.zoom * zoomFactor, 2));

                // Update zoom
                this.zoom = newZoom;

                this._updateTransform();
                this._updateConnections();
                return;
            }

            this.offset.x += e.deltaX;
            this.offset.y += e.deltaY;
            this.lastPanPoint = { x: e.clientX, y: e.clientY };
            this._updateTransform();
        }
    }

    /**
     * Open context menu
     * @param {MouseEvent} e 
     */
    contextMenuEventCb(e) {

        if (!e.target.closest('.component_picker')) {
            e.preventDefault();
            if (this.contextMenu && this.container.contains(this.contextMenu)) {
                this.container.removeChild(this.contextMenu);
            }

            this.contextMenu = new ContextMenu(this);
            this.container.appendChild(this.contextMenu);

            this.contextMenu.container.style.left = `${(this.worldMousePos.x * this.zoom) - this.offset.x * this.zoom - 20}px`;
            this.contextMenu.container.style.top = `${(this.worldMousePos.y * this.zoom) - this.offset.y * this.zoom - 20}px`;

            this.contextMenu.onZoomIn(this._zoomIn.bind(this));
            this.contextMenu.onZoomOut(this._zoomOut.bind(this));
        }
    }

    _startSelection(x, y) {
        this.selectionStart = { ...this.worldMousePos };
        this.selectionRect = document.createElement('div');
        this.selectionRect.className = 'selection-rect';
        this.selectionRect.style.position = 'absolute';
        this.selectionRect.style.border = '1px dashed #000';
        this.selectionRect.style.backgroundColor = 'rgba(0, 0, 255, 0.1)';
        this.container.appendChild(this.selectionRect);
    }

    _updateSelection(x, y) {
        const currentPos = { ...this.worldMousePos };
        const left = Math.min(this.selectionStart.x, currentPos.x);
        const top = Math.min(this.selectionStart.y, currentPos.y);
        const width = Math.abs(currentPos.x - this.selectionStart.x)
        const height = Math.abs(currentPos.y - this.selectionStart.y)

        this.selectionRect.style.left = `${(left - this.offset.x) * this.zoom}px`;
        this.selectionRect.style.top = `${(top - this.offset.y) * this.zoom}px`;
        this.selectionRect.style.width = `${width * this.zoom}px`;
        this.selectionRect.style.height = `${height * this.zoom}px`;
    }

    _endSelection() {
        const rect = this.selectionRect.getBoundingClientRect();
        const selectedNodes = this._getNodesInRect(
            rect.left, rect.top,
            rect.right, rect.bottom
        );

        if (selectedNodes.length > 0) {
            const group = new Group("Group title (edit me)", "Description (edit me)", [
                ...selectedNodes
            ]);

            this.addGroup(group);

            setTimeout(()=> this.drawGroup(group, rect), 0);

            return;
        }
        this.container.removeChild(this.selectionRect);
        this.selectionRect = null;
        this.selectionStart = null;

    }

    /**
     * 
     * @param {Group} group 
     * @param {DOMRect} rect
     */
    drawGroup(group, rect) {
       

        const width = rect.offsetWidth;
        const height = rect.offsetHeight;
        if(this.selectionRect)
            this.container.removeChild(this.selectionRect);
        this.selectionRect = null;
        this.selectionStart = null;
        const groupRect = group.nodesContainer.getBoundingClientRect();
        group.offsetNodePositions({
            x: ((rect.left - groupRect.left) / this.zoom),
            y: ((rect.top - groupRect.top) / this.zoom)
        }, this.zoom);

        const box = group.container;
        box.style.width = `${width * this.zoom}px`;
        box.style.height = `${height * this.zoom}px`;
        box.style.left = `${(rect.left + this.offset.x - (80 / this.zoom)) * this.zoom}px`;
        box.style.top = `${(rect.top - 400) * this.zoom}px`;
        box.style.position = 'absolute';
        box.style.border = '1px dashed #333';
        box.style.borderRadius = '8px';
        box.style.padding = '20px';
        box.style.background = '#dedede40';

        group.position = { x: (rect.left + this.offset.x - (80 / this.zoom)) * this.zoom, y: (rect.top - 400) * this.zoom };
        group.updateGroupTransform(this.zoom, this.offset, false);
        const conns = [...this.connections];
        this.connections = [];
        this.connectionLine.innerHTML = ``;
        conns.forEach((conn) => {
            this.addConnection(conn._graphInfo.from, conn._graphInfo.to);
        });

    }

    // Add method to find nodes within a selection rectangle
    _getNodesInRect(x1, y1, x2, y2) {
        const left = Math.min(x1, x2);
        const right = Math.max(x1, x2);
        const top = Math.min(y1, y2);
        const bottom = Math.max(y1, y2);

        /**
         * @type {Array<Node>}
         */
        const nodes = Array.from(this.nodesContainer.querySelectorAll(`${Namespace}-node`));

        return nodes.filter(node => {
            const nodeRect = node.container.getBoundingClientRect();
            return nodeRect.left >= left && nodeRect.left <= right &&
                nodeRect.top >= top && nodeRect.bottom <= bottom
        });
    }

    _updateTransform() {
        const transform = `translate(${-this.offset.x}px, ${-this.offset.y}px) scale(${this.zoom, this.zoom})`;
        const bg = this.background;
        const patternGrid = bg.querySelector("#grid");
        const patternInnerGrid = bg.querySelector("#inner-grid");
        const transform10 = parseInt(this.zoom * 10);
        const transform100 = transform10 * 10;

        patternGrid.setAttribute('x', parseInt(-this.offset.x % transform100));
        patternGrid.setAttribute('y', parseInt(-this.offset.y % transform100));
        patternGrid.setAttribute('width', transform100);
        patternGrid.setAttribute('height', transform100);
        patternInnerGrid.setAttribute('width', transform10);
        patternInnerGrid.setAttribute('height', transform10);

        this.nodesContainer.style.transform = transform;
        // this.nodesContainer.style.transformOrigin = "center center";
        this._updateConnections();
    }

    /**
     * @param {{nodeId: string, port:string}} startInfo
     * @param {HTMLDivElement} startPort 
     * @param {{nodeId: string, port:string}} endInfo
     * @param {HTMLDivElement} endPort 
     * @returns {Connection} connection
     */
    _createConnection(startInfo, startPort, endInfo, endPort, emit = true) {
        const connection = new Connection(
            startPort,
            endPort.offsetLeft,
            endPort.offsetTop,
            this.connectionLine,
        );
        connection.setEndPort(endPort, this.zoom, this.offset);
        connection.graphInfo = {
            from: startInfo,
            to: endInfo
        };

        this.connections.push(connection);
        this._updateConnections();
        this.connectionLine.querySelectorAll("path").forEach((path) => {
            if (path.dataset["key"] === `${connection.graphInfo.from.nodeId}-${connection.graphInfo.from.port}-to-${connection.graphInfo.to.nodeId}-${connection.graphInfo.to.port}`) {
                path.addEventListener('click', (e) => {
                    e.preventDefault();
                    this.connectionLine.removeChild(e.target);
                });
            }
        });


        // Send event to worker
        if (emit) {
            /**
            * @typedef {object} ConnectionPoint
            * @property {string} actor
            * @property {string} port
            */
            /**
             * @typedef {object} Connector 
             * @property {ConnectionPoint} from
             * @property {ConnectionPoint} to
             */


            let fromNode = this.getNode(startInfo.nodeId);
            let toNode = this.getNode(endInfo.nodeId);
            /**
             * @type {Port}
             */
            let fromPort = fromNode.getPort(startInfo.port) ?? fromNode.getPortById(startInfo.portId);

            /**
            * @type {Port}
            */
            let toPort = toNode.getPort(endInfo.port) ?? toNode.getPortById(endInfo.portId);


            /**
             * @type {Connector}
             */
            const edge = {
                from: {
                    actor: fromPort.nodeId,
                    port: {
                        id: fromPort.id,
                        name: fromPort.name,
                        metadata: {
                            trait: fromPort.trait,
                            position: fromPort.position
                        }
                    },
                },
                to: {
                    actor: toPort.nodeId,
                    port: {
                        id: toPort.id,
                        name: toPort.name,
                        metadata: {
                            trait: toPort.trait,
                            position: toPort.position
                        }
                    },
                },
                metadata: {
                    svg: {
                        path: {
                            d: connection.element.getAttribute("d"),
                            fill: connection.element.getAttribute("fill"),
                            stroke: connection.element.getAttribute("stroke"),
                            strokeWidth: connection.element.getAttribute("stroke-width"),
                        }
                    }
                }
            }

            this.worker.postMessage({
                event: EditorEvents.ADD_EDGE, edge
            });
        }
        return connection
    }

    _updateConnections() {
        this.connections.forEach((connection) => connection.update(this.zoom, this.offset));
    }

    _updateGroups() {
        this.container.querySelectorAll('.group').forEach((group) => {
            group.offset = this.offset;
            group.updateGroupSize(this.zoom, this);
        });
    }

}

customElements.define(`${Namespace}-editor`, Editor);