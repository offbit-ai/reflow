import { EditorState } from "./store.js";
import { Namespace } from "./util.js";
import Icons from "./ui/icons.js";

export class Drawer extends HTMLElement {
    constructor() {
        super();
        this.isOpen = false;
    }

    static get styles() {
        return `
            .drawer {
                display:flex;
                position:relative;
                width: 5px;
                height: 100vh;
                user-select: none;
                flex-grow:1;
                background:#22222288;
                flex-direction: column;
            }

            .drawer > .container {
                width: 100%;
                height: 100%;
                background:#22222288;
                padding: 10px;
                padding-left: 15px;
                overflow-x: none;
                overflow-y: scroll;
            }

            .drawer > .header {
                background: #4a4a4a;
                width: 100%;
                padding: 10px;
                padding-left: 15px;
                color:#00000;
                font-size:0.8rem;
                font-weight: 600;
            }


            .resizer {
                width: 5px;
                height: 100vh;
                background-color: #4a4a4a;
                position: absolute;
                left: 0;
                top: 0;
                cursor: ew-resize;
            }

            .toggle{
                position: absolute;
                background-color: #4a4a4a;
                top: 0;
                left: -25px;
                width: 28px;
                height: 40px;
                border-top-left-radius: 6px;
                border-bottom-left-radius: 6px;
                cursor: pointer;
                box-shadow: -2px 0 3px 0.3px rgba(0, 0, 0, 0.3);
                display: flex;
                align-items: center;
                justify-content: center;
            }
            .toggle > div {
                height: 20px;
            }
            .toggle > div > svg {
                fill: #fafafa88;
                width: 20px;
                height: 20px;
            }
             
        `
    }

    connectedCallback() {
        this.renderRoot = this.attachShadow({ mode: 'open' });

        this.renderRoot.innerHTML = `
            <div class="drawer">
             <div class="header"><span>Properties</span></div>
            <div class="container">
            </div>
            ${this._resizer}
            ${this._toggle}
            </div>
       `;
        const style = document.createElement("style");
        style.textContent = Drawer.styles;
        this.renderRoot.appendChild(style);

        this.toggle.appendChild(Icons.rightOpenPanel);

        this._attachEvents();
    }




    /**
     * Container
     *
     * @readonly
     * @type {HTMLDivElement}
     */
    get container() {
        return this.renderRoot.querySelector(".drawer");
    }
    /**
     * Toggle
     *
     * @readonly
     * @type {HTMLDivElement}
     */
    get toggle() {
        return this.renderRoot.querySelector(".toggle");
    }

    get _resizer() {
        return `
            <div id="resizer" class="resizer"></div>
        `
    }
    get _toggle() {
        return `
            <div class="toggle">
                
            </div>
        `
    }

    _attachEvents() {
        this.container.addEventListener("mousedown", this._mouseDown.bind(this));
        document.addEventListener("mousemove", this._mouseMove.bind(this));
        document.addEventListener("mouseup", this._mouseUp.bind(this));
        this.toggle.addEventListener("click", (e) => {
            e.preventDefault();
            if (!this.isOpen) {
                this.container.style.width = "300px";
                this.container.style.transition = "width 0.5s";
                this.isOpen = true;
                EditorState.setState((state) => ({ ...state, drawerIsOpen: this.isOpen, drawerResized: true }));
            } else {
                this.container.style.width = "5px";
                this.container.style.transition = "width 0.5s";
                this.isOpen = false;
                EditorState.setState((state) => ({ ...state, drawerIsOpen: this.isOpen, drawerResized: true }));

                setTimeout(() => {
                    EditorState.setState((state) => ({ ...state, drawerIsOpen: this.isOpen, drawerResized: false }));
                }, 500);
            }
        });

        EditorState.subscribe((state) => {
            if (state.drawerIsOpen) {
                this.toggle.innerHTML = "";
                this.toggle.appendChild(Icons.rightClosePanel);
            } else {
                this.toggle.innerHTML = "";
                this.toggle.appendChild(Icons.rightOpenPanel);
            }
        });
    }
    /**
     * On Mouse Down
     * @param {MouseEvent} e 
     */
    _mouseDown(e) {
        const composedTarget = e.composedPath ? e.composedPath()[0] : e.target.closest('.drawer');
        if (composedTarget.classList.contains("resizer")) {
            e.preventDefault();
            this.resizing = true;
            const rect = this.container.getBoundingClientRect();
            this.container.style.transition = "width 0s";
            this.resizeStartWidth = rect.width;
            return;
        }
    }
    /**
     * On Mouse Move
     * @param {MouseEvent} e 
     */
    _mouseMove(e) {
        if (this.resizing && this.isOpen) {
            const rect = this.container.getBoundingClientRect();

            const newWidth = rect.width - e.movementX;

            this.container.style.width = newWidth + 'px';

            if (newWidth > 5) {
                this.toggle.style.setProperty("--caret", "'>'");
                this.isOpen = true;
                EditorState.setState((state) => ({ ...state, drawerIsOpen: this.isOpen, drawerResized: true }));
            } else if (newWidth <= 5) {
                this.toggle.style.setProperty("--caret", "'<'");
                this.isOpen = false;
                EditorState.setState((state) => ({ ...state, drawerIsOpen: this.isOpen, drawerResized: true }));
                setTimeout(() => {
                    EditorState.setState((state) => ({ ...state, drawerIsOpen: this.isOpen, drawerResized: false }));
                }, 500);
            }

            return;
        }
    }

    /**
     * On Mouse Up
     * @param {MouseEvent} e 
     */
    _mouseUp(e) {
        if (this.resizing) {
            this.resizing = false;
        }
    }

}


customElements.define(`${Namespace}-drawer`, Drawer);


