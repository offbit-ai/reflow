import { NodeRepository } from "./node-list-mock.js";
import { getTranslateXY, Namespace } from "./util.js";
import { Node } from "./node.js";

export class NodePicker extends HTMLElement {
    /**
     * 
     * @param {import("./editor").Editor} editor 
     *  @param {import("./node.js").Position} pos
     */
    constructor(editor, pos) {
        super();
        this.editor = editor;
        this.globalMousePos = pos;
    }

    static get styles() {
        return `
            .nodePicker {
                background:#dedede;
                position: absolute;
                box-shadow: 2px 2px 3px 1px rgba(0, 0, 0, 0.3);
                min-width: 400px;
                height: fit-content;
                padding-left: 0 !important;
                padding-right: 0 !important;
                padding-top: 10px;
                padding-bottom: 10px;
                top: 40px;
                border-radius: 8px;

            }
            .nodePicker > input {
                width: 400px;
                padding: 8px;
                border-radius: 8px;
                margin: 10px;
            }

            .nodePicker > ul {
                padding: 0;
                margin: 0;
                list-style-type: none;
            }

            .nodePicker > .list > .item {
                padding: 10px;
                border: 0.5px solid #2e2e2e80;
                cursor: pointer;
            }

            .item:hover {
                background: #2e2e2e80;
                color: #fff;
            }
            
        `
    }

    connectedCallback() {
        this.renderRoot = this.attachShadow({ mode: 'open' })
        this.renderRoot.innerHTML = `
            <dialog class="nodePicker">
               <input type="text" placeholder="search components..."></input>
               <ul class="list">
                    
               </ul>
            </dialog>
        `;
        const style = document.createElement('style');
        style.textContent = NodePicker.styles;
        this.renderRoot.appendChild(style);
        this.firstUpdated();
    }

    firstUpdated() {
        for (const node of NodeRepository) {
            const item = document.createElement("li");
            item.classList.add("item");
            const _node = Object.assign({}, node);
            item.innerText = node.name;
            this.list.appendChild(item);
            item.onmousedown = (e) => {
                const contOffset = { x: e.clientX+ (this.editor.offset.x / this.editor.zoom) - this.globalMousePos.x *this.editor.zoom, y: e.clientY + (this.editor.offset.y / this.editor.zoom) - this.globalMousePos.y };
                this.close();
                const __node = new Node({ ..._node, position: { ...contOffset } });
                this.editor.addNode(__node);
            }

        }
    }

    /**
     * @returns {HTMLDialogElement}
     */
    get container() {
        return this.renderRoot.querySelector(".nodePicker");
    }

    get list() {
        return this.renderRoot.querySelector(".list");
    }

    open() {
        this.container.open = true;
    }

    close() {
        if (this.container.open) {
            this.container.close();
        }
    }
}

customElements.define(`${Namespace}-node-picker`, NodePicker);