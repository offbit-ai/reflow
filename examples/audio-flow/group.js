import { uuidv4 } from './uuid/v4.js';
import { Node } from './node.js';
import { Namespace, debounce } from "./util.js";
import { EditorEvents } from './events.js';


/**
 * @typedef {object} Position
 * @property {number} x 
 * @property {number} y
 */


/**
 * @typedef {object} NodeOptions
 * @property {string} name
 * @property {string} description
 * @property {Array<PortOptions>} inports 
 * @property {Array<PortOptions>} outports 
 * @property {Position} position
 */


export class Group extends HTMLElement {

    /**
     * @type {Array<Node>}
     */
    nodes;

    /**
     * @type {Array<string>}
     */
    nodeIds;

    /**
     * @type {string}
     */
    name;

    /**
     * @type {string}
     */
    description

    /**
     * @type {string}
    */
    _id;

    /**
     * @type {Position}
     */
    _position = { x: 0, y: 0 };

    get position() {
        return this._position;
    }

    set position(pos) {
        this._position = pos;
    }

    /**
     * Create a group
     * @param {string} name 
     * @param {string} description 
     * @param {Array<Node>} nodes 
     */
    constructor(name = '', description = '', nodes = [], id =  null) {
        super();
        this.nodes = nodes;
        this.description = description;
        this.name = name;
        this.nodeIds = [];
        this._id = id ?? uuidv4().replaceAll("-", "");
        this.worldMousePos = { x: 0, y: 0 };
        this.dragOffset = { x: 0, y: 0 };
    }

    static get styles() {
        return `
            .node-group{

            }
            .node-group:hover {
                cursor: grab;
            }
            .node-group-header {
                position: absolute;
                z-index: 20;
                display: flex;
                height: 70px;
                justify-content: space-between;
                flex-direction: column;
            }
            .node-group-header > div,h4,span {
                pointer-events: none;
            }

            .node-group-name{
                font-weight: 700;
                font-size: 0.8rem;
                appearance: none;
                border: none;
            }

            .node-group-description {
                font-weight: 300;
                font-size: 0.7rem;
                appearance: none;
                border: none;
            }
            
            .node-group-name, .node-group-description {
                background-color: white;
                padding: 8px;
                box-shadow: 2px 2px 3px 1px rgba(0, 0, 0, 0.3);
            }
        `
    }

    connectedCallback() {
        this.renderRoot = this.attachShadow({ mode: 'open' })
        this.renderRoot.innerHTML = `
            <div class="node-group" id=${this.id}>
               <div class="node-group-header">
                <input class="node-group-name" type="text" value="${this.name}" />
                <input class="node-group-description" type="text" value="${this.description}" />
               </div>
                <div class="node-group-nodes-container"></div>
            </div>
        `;
        const style = document.createElement('style');
        style.textContent = Group.styles;
        this.renderRoot.appendChild(style);
        this.firstUpdated();
    }

    /**
     * @readonly
     */
    get id() {
        return this._id;
    }

    get container() {
        return this.renderRoot.querySelector('.node-group');
    }

    get groupNameInput() {
        return this.renderRoot.querySelector('.node-group-name');
    }
    get groupDescInput() {
        return this.renderRoot.querySelector('.node-group-description');
    }

    /**
     * @type {HTMLDivElement}
     */
    get nodesContainer() {
        return this.renderRoot.querySelector('.node-group-nodes-container');
    }

    firstUpdated() {
        this.groupNameInput.style.width = this.groupNameInput.value.length + 1 + "ch";
        this.groupDescInput.style.width = this.groupDescInput.value.length + 1 + "ch";
        this.groupNameInput.addEventListener('keyup', (e)=>{
            this.groupNameInput.style.width = e.target.value.length + "ch";
        });
        this.groupDescInput.addEventListener('keyup', (e)=>{
            this.groupDescInput.style.width = e.target.value.length + "ch";
        });

        /**
         * @type Array<string>
         */
        const ids = this.nodes.reduce((prev, next) => {
            return [...prev, this.addNode(next)]
        }, []);
        this.nodeIds = [...this.nodeIds, ...ids];

        this.container.addEventListener("mousedown", (e)=> {
           if(!e.composedPath()[0].classList.contains("node-container")) {
            this.container.style.outline = "solid 5px skyblue";
           } else {
            this.container.style.outline = "none";
           }
        });
    }

    debouncedMetadata = debounce((data) => {
        this.worker?.postMessage(({event: EditorEvents.UPDATE_GROUP_METADATA, data}))
    }, 10);

    /**
     * @param {Worker} worker 
     */
    registerUpdateEvents(worker){
        this.worker = worker;
        
        this.groupNameInput.addEventListener('keyup', (e)=>{
            this.debouncedMetadata({id: this.id, name: e.target.value});
        });
        this.groupDescInput.addEventListener('keyup', (e)=>{
            this.debouncedMetadata({id: this.id, description: e.target.value});
        });
    }


    /**
     * Add Node
     * @param {import('./node.js').Node} node 
     * @returns {string}
     */
    addNode(node) {
        node.setAttribute("class", "node");
        node.setAttribute("data-group", this.id);
        this.nodesContainer.appendChild(node);
        this.nodeIds.push(node.id);
        return node.id;
    }

    /**
     * Get a node
     * @param {string} id
     */
    getNode(id) {
        let _nodeRoots = this.nodesContainer.querySelectorAll(`${Namespace}-node`);
        /**
         * @type {Node}
         */
        const node = Array.from(_nodeRoots).find((node) => node.id === id);
        return node
    }


    /**
     * @type Position
     */
    dragOffset;

    /**
     * Initiate the drag
     * @param {number} x 
     * @param {number} y 
     * @param {Position} offset
     */
    startDrag(x, y, offset) {
        this.dragOffset = {
            x: x - (this.position.x - offset.x),
            y: y - (this.position.y - offset.y)
        };
    }

    /**
     * Drag the node
     * @param {number} x 
     * @param {number} y 
     * @param {Position} offset
     */
    drag(x, y, offset) {
        this.position.x = x - this.dragOffset.x + offset.x;
        this.position.y = y - this.dragOffset.y + offset.y;

        this.container.style.left = `${this.position.x}px`;
        this.container.style.top = `${this.position.y}px`;

        if(this.worker){
            this.debouncedMetadata({id: this.id, position: this.position});
        }
        return;
    }

    /**
     * Track the world mouse position
     * @param {number} x 
     * @param {number} y 
     */
    updateWorldMousePos(x, y) {
        this.worldMousePos.x = x;
        this.worldMousePos.y = y;
    }

    /**
     * Update the group's transform
     * @param {number} zoom 
     * @param {Position} offset
     * @param {boolean} withPosition 
     */
    updateGroupTransform(zoom, offset, withPosition) {
        setTimeout(() => {
            const rect = this.container.getBoundingClientRect();
            const elements = this.nodesContainer.querySelectorAll('.node');

            let minX = (rect.left) / zoom, minY = (rect.top) / zoom, maxX = -Infinity, maxY = -Infinity;

            elements.forEach(el => {
                const rect = el.shadowRoot.querySelector('.node-container').getBoundingClientRect();
                minX = Math.min(minX, rect.left / zoom);
                minY = Math.min(minY, rect.top / zoom);
                maxX = Math.max(maxX, rect.right / zoom);
                maxY = Math.max(maxY, rect.bottom / zoom);
            });


            const box = this.container;
            box.style.width = `${((maxX - minX))}px`;
            box.style.height = `${((maxY - minY))}px`;
            if (withPosition) {
                box.style.left = `${minX + offset.x}px`;
                box.style.top = `${minY + offset.y}px`;
            }
            box.style.position = 'absolute';
            box.style.border = '1px dashed #333';
            box.style.borderRadius = '8px';
            box.style.padding = '20px';
            box.style.background = '#dedede80';

        }, 0);
    }

    /**
     * Update the group's transform, size only
     * @param {number} zoom 
     * @param {Editor} editor 
     */
    updateGroupSize(zoom, editor) {
        setTimeout(() => {
            if (editor.draggingNode.dataset['group'] === this.id) {
                const box = this.container;
                const rect = box.getBoundingClientRect();
                const elements = this.nodesContainer.querySelectorAll('.node');

                const activeRect = editor.draggingNode.shadowRoot.querySelector('.node-container').getBoundingClientRect();

                const padding = 20 / zoom;
                let minX = (rect.left / zoom), minY = (rect.top / zoom), maxX = -Infinity, maxY = -Infinity;

                elements.forEach(el => {
                    const _rect = el.shadowRoot.querySelector('.node-container').getBoundingClientRect();
                    minX = Math.min(minX, _rect.left / zoom);
                    minY = Math.min(minY, _rect.top / zoom);
                    maxX = Math.max(maxX, _rect.right / zoom);
                    maxY = Math.max(maxY, _rect.bottom / zoom);
                });

                let activePosX = Math.abs(Math.round((rect.left / zoom) - (activeRect.left / zoom))) - padding;
                let activePosY = Math.abs(Math.round((rect.top / zoom) - (activeRect.top / zoom))) - padding;

                if (activePosX >= 1) {
                    box.style.width = `${(maxX - minX)}px`;
                } else {
                    editor.draggingNode.position.x = (editor.draggingNode.position.x) + 10;
                    editor.draggingNode.updatePosition();
                    editor.draggingNode = null;
                    editor.draggingGroup = this;
                    this.startDrag(this.worldMousePos.x, this.worldMousePos.y, editor.offset);
                    activePosX = 0;
                }

                if (activePosY >= 1) {
                    box.style.height = `${(maxY - minY)}px`;
                } else {
                    editor.draggingNode.position.y = (editor.draggingNode.position.y) + 10;
                    editor.draggingNode.updatePosition();
                    editor.draggingNode = null;
                    editor.draggingGroup = this;
                    this.startDrag(this.worldMousePos.x, this.worldMousePos.y, editor.offset);
                    activePosY = 0;
                }
            }
        }, 0);
    }

    /**
     * 
     * @param {Position} offset
     */
    offsetNodePositions(offset) {
        /**
         * @type {Array<Node>}
         */
        const nodes = Array.from(this.nodesContainer.querySelectorAll('.node'));
        nodes.forEach((node) => {
            node.position.x = node.position.x - offset.x;
            node.position.y = node.position.y - offset.y;
            node.container.style.transform = `translate(${node.position.x}px, ${node.position.y}px)`;
            node.container.style.transformOrigin = "center center";
        });
    }
}


customElements.define(`${Namespace}-group`, Group);