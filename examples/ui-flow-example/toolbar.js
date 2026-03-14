import { Namespace } from "./util.js";
import "./editor.js";
import { EditorState } from "./store.js";
import Icons from "./ui/icons.js"


export class Toolbar extends HTMLElement {
    constructor() {
        super();


        this.renderRoot = this.attachShadow({ mode: 'open' });

        this.renderRoot.innerHTML = `
            <div class="toolbar">
                <ul class="menu">
                    <li id="graph_editor" class="active">Graph Editor</li>
                    <button class="collapse_toggle">
                    </button>
                </ul>
                <div class="active_container">
                </div>
                ${this._resizer}
            </div>
       `;
        const style = document.createElement("style");
        style.textContent = Toolbar.styles;
        this.renderRoot.appendChild(style);


        this.renderRoot.querySelector(".menu > .collapse_toggle").appendChild(Icons.topClosePanel);
    }

    static get styles() {
        return `
            .toolbar{
                position: relative;
                width: 100%;
                display: flex;
                flex-direction: column;
                height: 40vh;
                overflow: hidden;
                user-select: none;
                flex-shrink:1;
                background:#22222288;
                flex-direction:column;
            }

            .resizer {
                width: 100%;
                height: 10px;
                background-color: #4a4a4a;
                position: absolute;
                left: 0;
                top: 0;
                cursor: ns-resize;
            }

            .menu {
                position:relative;
                display: flex;
                width: 100%;
                background-color: #4a4a4a;
                margin:0;
                list-style-type: none;
                height: 45px;
            }
            
            .menu > li {
                color:#000;
                font-size:0.8rem;
                font-weight: 600;
                cursor: pointer;
                display: flex;
                align-items: center;
                justify-content: center;
                background:rgba(91, 91, 91, .5);
                padding: 10px;
                height: 40px;
            }
            
            .menu > li.active {
                background:rgb(91, 91, 91);
                border-top-left-radius: 8px;
                border-top-right-radius: 8px;
                box-shadow: 0px -3px 3px 0.3px rgba(0, 0, 0, 0.3);
            }

            .active_container {
                display:block;
                width: 100%;
                min-height:38vh;
                flex-grow:1;
                overflow: hidden;
            }

            .collapse_toggle {
                display:flex;
                align-items: center;
                height: 100%;
                position: absolute;
                right: 45px;
                top: 2px;
                background: transparent;
                outline: none;
                border: none;
                cursor: pointer;
            }
            
            .collapse_toggle > div > svg > path {
                width: 24px;
                height: 24px;
                fill: #fafafa88;
            }
        `
    }

    connectedCallback() {

        this._attachEvents();
    }

    get _resizer() {
        return `
            <div id="resizer" class="resizer"></div>
        `
    }

    get toolbar() {
        return this.renderRoot.querySelector(".toolbar")
    }

    get container() {
        return this.renderRoot.querySelector(".active_container")
    }

    _attachEvents() {
        EditorState.subscribe((state) => {
            if (!state.toolbarCollapsed) {
                this.renderRoot.querySelector(".collapse_toggle").innerHTML = "";
                this.renderRoot.querySelector(".collapse_toggle").appendChild(Icons.topClosePanel);
                this.toolbar.style.transition = "height 0.5s";
                this.toolbar.style.height = "40vh";
            } else {
                this.renderRoot.querySelector(".collapse_toggle").innerHTML = "";
                this.renderRoot.querySelector(".collapse_toggle").appendChild(Icons.topOpenPanel);
                this.toolbar.style.transition = "height 0.5s";
                this.toolbar.style.height = "45px";
            }
        });
        this.renderRoot.querySelector(".collapse_toggle").addEventListener("mousedown", (e) => {
            e.preventDefault();
            EditorState.setState((state) => ({ ...state, toolbarCollapsed: !EditorState.getState().toolbarCollapsed }));
        });

        /**
        * @type {HTMLLIElement}
        */
        const activeMenu = this.renderRoot.querySelector("li.active");
        if (activeMenu) {
            this.insertActiveContainer(activeMenu);
        }

        this.renderRoot.addEventListener("mousedown", (e) => {
            const composedTarget = e.composedPath ? e.composedPath()[0] : e.target.closest('.toolbar');
            const rect = this.toolbar.computedStyleMap();
       
            if (composedTarget.classList.contains("resizer") && rect.get("height").value >= 200) {
                e.preventDefault();
                this.resizing = true;
                this.toolbar.style.transition = "height 0s";
                this.resizeStartHeight = rect.height;
                return;
            }
        });
        this.renderRoot.addEventListener("mousemove", (e) => {
            if (this.resizing) {
                const rect = this.toolbar.getBoundingClientRect();

                const newHeight = rect.height - e.movementY;
                if (newHeight >= 200) { 
                    this.toolbar.style.height = newHeight + 'px';
                    this._resize();
                    return;
                }
                return;
            }
        });
        document.addEventListener("mouseup", (e) => {
            if (this.resizing) {
                this.resizing = false;
                return;
            }
        });
        this.renderRoot.addEventListener("mouseup", (e) => {
            if (this.resizing) {
                this.resizing = false;
                return;
            }
        });
        this.renderRoot.querySelectorAll(".menu > li").forEach((element) => {
            /**
             * @type {HTMLLIElement}
             */
            const menu = element;
            menu.addEventListener("mousedown", (e) => {
                e.preventDefault();
                this.insertActiveContainer(menu);
            });
        });
    }

    /**
    * @param {HTMLLIElement} menu
    */
    insertActiveContainer(menu) {
        if (EditorState.getState().toolbarCollapsed) {
            EditorState.setState((state) => ({ ...state, toolbarCollapsed: false }));
        }
        switch (menu.id) {
            case "graph_editor": {
                if (!this.container.querySelector(`${Namespace}-editor`)) {
                    /**
                     * @type {import("./editor.js").Editor}
                     */
                    const editor = document.createElement(`${Namespace}-editor`);
                    this.container.appendChild(editor);
                    menu.classList.add("active");
                }

                break;
            }
        }
    }

    _resize() {
        /**
        * @type {HTMLLIElement}
        */
        const menu = this.renderRoot.querySelector("li.active");
        switch (menu.id) {
            case "graph_editor": {
                /**
                * @type {import("./editor.js").Editor}
                */
                const editor = this.container.querySelector(`${Namespace}-editor`);
                editor._resize();
                break;
            }
        }
    }

}

customElements.define(`${Namespace}-toolbar`, Toolbar);
