import { Namespace } from './util.js';
import { uuidv4 } from './uuid/v4.js';
import { Port } from './port.js';
import { getGraphWorker } from './util.js';
import { EditorEvents } from './events.js';

/**
 * @typedef {object} Position
 * @property {number} x 
 * @property {number} y
 */




/**
 * @typedef {object} NodeOptions
 * @property {string} name
 * @property {string | undefined} id
 * @property {string} process
 * @property {string} description
 * @property {Array<Port>} inports 
 * @property {Array<Port>} outports 
 * @property {Position} position
 * @property {number} index
 */


export class Node extends HTMLElement {
    /**
     * @type {string}
    */
    _id;

    /**
     * @type {string}
     */
    _name;

    /**
     * @type {Array<Port>}
     */
    _inports;

    /**
     * @type {Array<Port>}
     */
    _outports;

    /**
     * @type {Position}
     */
    _position;

    /**
     * @readonly
     * @type {string}
     */
    process

    _index = 0;

    /**
     * @param {NodeOptions} options 
     */
    constructor(options) {
        super();
        this._id = options.id ?? uuidv4().replaceAll("-", "");
        this._name = options?.name ?? '';
        this._inports = options?.inports.map((_p) => new Port({ ..._p, direction: "in", nodeId: this.id })) ?? [];
        this._outports = options?.outports.map((_p) => new Port({ ..._p, direction: "out", nodeId: this.id  })) ?? [];
        this.ports = this._inports.concat(this.outports);
        this._position = options?.position ?? { x: 10, y: 10 };
        this.dragOffset = { x: 0, y: 0 };
        this.process = options.process;
        this._index = options.index ?? 0;


        this.worker = getGraphWorker();

        this.renderRoot = this.attachShadow({ mode: "open" });
        this.setAttribute("id", this.id);
        this.setAttribute("data-options", JSON.stringify(options));
        this.renderRoot.innerHTML = `
            <div style='transform:translate(${this.position.x}px, ${this.position.y}px)' id=${this.id} class="node-container" tabindex="${this._index}">
                <div class="label-container"><label>${this.name}</label></div>
                <div class="ports-wrapper">
                    <div class="inport-container ports">
                    </div>
                    <div class="outport-container ports">
                    </div>
                </div>
            </div>
        `;
        const style = document.createElement('style');
        style.textContent = Node.styles;
        this.renderRoot.appendChild(style);
        const inportsContainer = document.createDocumentFragment();

        this.inports?.forEach(((port) => {
            port.direction = "in";
            port._nodeId = this.id;
            if (!document.getElementById(port.id)) {
                inportsContainer.appendChild(port);
            }
        }).bind(this));

        const outportsContainer = document.createDocumentFragment();
        this.outports?.forEach(((port) => {
            port.direction = "out";
            port._nodeId = this.id;
            if (!document.getElementById(port.id)) {
                outportsContainer.appendChild(port);
            }
        }).bind(this));
        this.inportsContainer.appendChild(inportsContainer);
        this.outportsContainer.appendChild(outportsContainer);
    }


    /**
     * @readonly
     */
    get id() {
        return this._id;
    }

    get inports() {
        return this._inports
    }
    set inports(ports) {
        this._inports = ports;

       
        const inportsContainer = document.createDocumentFragment();

        this.inports?.forEach(((port) => {
            port.direction = "in";
            port._nodeId = this.id;
            if (!document.getElementById(port.id)) {
                inportsContainer.appendChild(port);
            }
        }).bind(this));
        this.inportsContainer.replaceChildren(...[]);
        this.inportsContainer.appendChild(inportsContainer);
    }

    get outports() {
        return this._outports
    }
    set outports(ports) {
        this._outports = ports;
        const outportsContainer = document.createDocumentFragment();

        this.outorts?.forEach(((port) => {
            port.direction = "out";
            port._nodeId = this.id;
            if (!document.getElementById(port.id)) {
                inportsContainer.appendChild(port);
            }
        }).bind(this));
        this.outportsContainer.replaceChildren(...[]);
        this.outportsContainer.appendChild(outportsContainer);
    }

    get name() {
        return this._name
    }
    set name(_n) {
        this._name = _n;
    }

    get position() {
        return this._position;
    }

    set position(pos) {
        this._position = pos;
    }

    set index(i){
        this.container.setAttribute("tabindex", parseInt(i))
    }

    /**
     * @param {string} name
     */
    getInport(name) {
        return this.inports.find((port) => port.name === name)
    }

    /**
     * @param {string} name 
     */
    getOutport(name) {
        return this.outports.find((port) => port.name === name)
    }

    /**
     * Get a port
     * @param {string} name
     * @returns {Port}
     */
    getPort(name) {
        return this.ports.find((port) => port.name === name)
    }

    /**
     * Get a port by ID
     * @param {string} id
     * @returns {Port}
     */
    getPortById(id) {
        return this.ports.find((port) => port.id === id);
    }

    static get styles() {
        return `
            .node-container {
                position: absolute;
                min-width: 150px;
                height: fit-content;
                background-color: #f0f0f0;
                border: 0.5px solid #333;
                border-radius: 5px;
                display: flex;
                flex-direction: column;
                justify-content: flex-start;
                align-items: center;
                cursor: move;
                box-shadow: 2px 1px 5px 0.5px rgba(0, 0, 0, 0.2);
            }
            .node-container:focus {
                outline: solid 3px #222;
            }
            .label-container {
                display: flex;
                padding: 1px;
                justify-content: center;
                align-items: center;
                pointer-events: none;
                width: 100%;
                font-size: 0.8rem;
                background-color: rgba(0, 0, 0, 0.4);
                border-bottom: 1px solid rgba(0, 0, 0, 0.2);
                margin-bottom: 5px;
                border-top-left-radius: 5px;
                border-top-right-radius: 5px;
                color: #fff;
                font-weight: 600;
                text-shadow:
                0.5px 0.5px 0 rgba(0, 0, 0, 0.1),
                -0.5px 0.5px 0 rgba(0, 0, 0, 0.1),
                -0.5px -0.5px 0 rgba(0, 0, 0, 0.1),
                0.5px -0.5px 0 rgba(0, 0, 0, 0.1);
            }
            .inport-container {
                display: flex;
                justify-content: start;
                flex-direction: column;
                height: inherit;
                padding-left: 0.2rem;
            }
            .outport-container {
                display: flex;
                justify-content: start;
                flex-direction: column;
                align-self: flex-end;
                height: inherit;
                padding-right: 0.2rem;
            }
            .ports-wrapper{
                display: flex;
                width: 100%;
                justify-content: space-between;
                padding-bottom: 0.5rem; 
            }
        `
    }

    connectedCallback() {
        // if (!this.renderRoot) {
        //     this.renderRoot = this.attachShadow({ mode: 'open' });
        // }

        this.firstUpdated();
    }

    get container() {
        return this.renderRoot.querySelector('.node-container');
    }

    get portsContainer() {
        return this.renderRoot.querySelector('.ports');
    }

    get inportsContainer() {
        return this.renderRoot.querySelector('.inport-container');
    }
    get outportsContainer() {
        return this.renderRoot.querySelector('.outport-container');
    }

    firstUpdated() {
        this.ports.forEach((port)=> port.updateCallback = ((value) =>{
            const options = this.getMetadata();
            this.worker.postMessage({
                event: EditorEvents.UPDATE_NODE_METADATA, node: {
                    id: this.id, process: this.process, metadata: {
                        index: this.index,
                        ...options
                    }
                }
            });
        }).bind(this));

        this.container.addEventListener("mousedown", ()=> this.container.focus());
    }

    /**
     * @type {Array<Port>}
     */
    ports;

    /**
     * 
     * @param {PortOptions} port 
     */
    createPort(port) {
        /**
         * @type {Port}
         */
        let _port = new Port(); //document.createElement('audio-flow-port');
        _port.direction = port.direction;
        _port.name = port.name;
        _port.position = port.position;
        _port.nodeId = port.nodeId;
        this.ports.push(_port);
        return _port
    }

    /**
     * @type Position
     */
    dragOffset;

    /**
     * Initiate the drag
     * @param {number} x 
     * @param {number} y 
     */
    startDrag(x, y) {
        this.dragOffset = {
            x: x - this.position.x,
            y: y - this.position.y
        };
    }

    /**
     * Drag the node
     * @param {number} x 
     * @param {number} y 
     */
    drag(x, y) {
        this.position.x = (x - this.dragOffset.x);
        this.position.y = (y - this.dragOffset.y);
        this.updatePosition();
        return;
    }

    updatePosition() {
        this.container.style.transform = `translate(${this.position.x}px, ${this.position.y}px)`;
        this.container.style.transformOrigin = "center center";
    }

    getMetadata() {
        /**
         * @type {NodeOptions}
         */
       let metadata = JSON.parse(this.getAttribute("data-options"));
       metadata.position = {...this.position};
       
       metadata.inports = this.inports.map((port) => port.getMetadata()),
       metadata.outports = this.outports.map((port) => port.getMetadata())
       return metadata
    }

}



customElements.define(`${Namespace}-node`, Node);