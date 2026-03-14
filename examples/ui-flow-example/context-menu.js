import { NodePicker } from "./node-picker.js";
import { Namespace } from "./util.js";

export class ContextMenu extends HTMLElement {
    /**
     * 
     * @param {import("./editor").Editor} editor 
     */
    constructor(editor) {
        super();
        this.editor = editor;
    }

    static get style() {
        return `
            .contextMenu{
                position:absolute;
                width: max-content;
                height: fit-content;
                background-color: #fefefe;
                border: 1px solid #000;
                box-shadow: 2px 2px 3px 1px rgba(0, 0, 0, 0.3);
                display: flex;
                z-index: 40;
                list-style-type: none;
                justify-content: stretch;
                flex-direction: column;
                padding: 0;
                margin:0;
            }

            
            .list-wrapper {
                height: fit-content;
                border-bottom: 0.5px solid #1D1D1D;
                width: inherit;
                padding: 5px;
                display: flex;
                flex-direction: row;
                justify-content: space-between;
                align-items: center;
            }

            .list-wrapper span {
                margin-right: 10px;
                font-size: 0.85rem;
            }

            .list-wrapper > svg {
                width: 14px;
                height: 14px;
                align-self: flex-end;
            }
            
            .list-wrapper:hover {
                background: #12121266;
                cursor: pointer;
            }
            
            .list-wrapper:active {
                background: #12121244;
                cursor: pointer;
            }

            .list-wrapper:hover path {
                fill: #fefefe
            }

            path {
                fill:#1D1D1D
            }
            
            .list-wrapper:hover > span {
                color: #fefefe;
                text-outline: #000;
            }
        `
    }

    connectedCallback() {
        this.renderRoot = this.attachShadow({ mode: 'open' });
        this.renderRoot.innerHTML = `
            <ul class="contextMenu">
                <li class="add-node">
                    <div class="list-wrapper">
                        <span>Add node</span>
                        <svg viewBox="0 0 20 20" version="1.1" xmlns="http://www.w3.org/2000/svg" xmlns:xlink="http://www.w3.org/1999/xlink">
                            <g id="Icons" stroke="none" stroke-width="1" fill="none" fill-rule="evenodd">
                                <g id="Rounded" transform="translate(-646.000000, -2682.000000)">
                                    <g id="Image" transform="translate(100.000000, 2626.000000)">
                                        <g id="-Round-/-Image-/-add_to_photos" transform="translate(544.000000, 54.000000)">
                                            <g>
                                                <polygon id="Path" points="0 0 24 0 24 24 0 24"></polygon>
                                                <path d="M3,6 C2.45,6 2,6.45 2,7 L2,20 C2,21.1 2.9,22 4,22 L17,22 C17.55,22 18,21.55 18,21 C18,20.45 17.55,20 17,20 L5,20 C4.45,20 4,19.55 4,19 L4,7 C4,6.45 3.55,6 3,6 Z M20,2 L8,2 C6.9,2 6,2.9 6,4 L6,16 C6,17.1 6.9,18 8,18 L20,18 C21.1,18 22,17.1 22,16 L22,4 C22,2.9 21.1,2 20,2 Z M18,11 L15,11 L15,14 C15,14.55 14.55,15 14,15 C13.45,15 13,14.55 13,14 L13,11 L10,11 C9.45,11 9,10.55 9,10 C9,9.45 9.45,9 10,9 L13,9 L13,6 C13,5.45 13.45,5 14,5 C14.55,5 15,5.45 15,6 L15,9 L18,9 C18.55,9 19,9.45 19,10 C19,10.55 18.55,11 18,11 Z" id="Icon-Color"></path>
                                            </g>
                                        </g>
                                    </g>
                                </g>
                            </g>
                        </svg>
                        
                    </div>
                </li>
                <li class="zoom-in">
                    <div class="list-wrapper">
                    <span>Zoom in</span>
                        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M10.035 18.069a7.981 7.981 0 0 0 3.938-1.035l3.332 3.332a2.164 2.164 0 0 0 3.061-3.061l-3.332-3.332a8.032 8.032 0 0 0-12.68-9.619 8.034 8.034 0 0 0 5.681 13.715zM5.768 5.768A6.033 6.033 0 1 1 4 10.035a5.989 5.989 0 0 1 1.768-4.267zm.267 4.267a1 1 0 0 1 1-1h2v-2a1 1 0 0 1 2 0v2h2a1 1 0 0 1 0 2h-2v2a1 1 0 0 1-2 0v-2h-2a1 1 0 0 1-1-1z"/></svg>
                    </div>
                </li>
                <li class="zoom-out">
                    <div class="list-wrapper">
                        <span>Zoom out</span>
                        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M10.035 18.069a7.981 7.981 0 0 0 3.938-1.035l3.332 3.332a2.164 2.164 0 0 0 3.061-3.061l-3.332-3.332a8.032 8.032 0 0 0-12.68-9.619 8.034 8.034 0 0 0 5.681 13.715zM5.768 5.768A6.033 6.033 0 1 1 4 10.035a5.989 5.989 0 0 1 1.768-4.267zm.267 4.267a1 1 0 0 1 1-1h6a1 1 0 0 1 0 2h-6a1 1 0 0 1-1-1z"/></svg>
                    </div>
                </li>
            </ul>
        `
        const style = document.createElement('style');
        style.textContent = ContextMenu.style;
        this.renderRoot.appendChild(style);
        const picker = new NodePicker(this.editor, this.container.getBoundingClientRect());

        setTimeout(() => {
            this.container.querySelector(".add-node").addEventListener("click", (e) => {
                e.preventDefault();
                this.editor.container.removeChild(this);
                this.editor.container.appendChild(picker);
                picker.open();


                this.editor.container.addEventListener("click", (e)=>{
                    if(e.target.classList.contains("editor")){
                        picker.close();
                    } 
                });
            });
        }, 0)
    }

    get container() {
        return this.renderRoot.querySelector('.contextMenu')
    }

    /**
     * @type {HTMLDivElement}
     */
    get plus() {
        return this.renderRoot.querySelector('.zoom-in');
    }

    /**
     * @type {HTMLDivElement}
     */
    get minus() {
        return this.renderRoot.querySelector('.zoom-out');
    }

    /**
     * 
     * @param {Function} callback 
     */
    onZoomIn(callback) {
        this.plus.addEventListener('click', callback);
    }

    /**
     * 
     * @param {Function} callback 
     */
    onZoomOut(callback) {
        this.minus.addEventListener('click', callback);
    }
}

customElements.define(`${Namespace}-context-menu`, ContextMenu);