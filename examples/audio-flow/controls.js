import { Namespace } from "./util.js";

export class Controls extends HTMLElement {

    constructor() {
        super();

        this.renderRoot = this.attachShadow({ mode: 'open' });

        this.renderRoot.innerHTML = `
            <div class="controls">
                <div class="zoom-control">
                    <button class="plus" aria-label="zoom in">
                        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M10.035 18.069a7.981 7.981 0 0 0 3.938-1.035l3.332 3.332a2.164 2.164 0 0 0 3.061-3.061l-3.332-3.332a8.032 8.032 0 0 0-12.68-9.619 8.034 8.034 0 0 0 5.681 13.715zM5.768 5.768A6.033 6.033 0 1 1 4 10.035a5.989 5.989 0 0 1 1.768-4.267zm.267 4.267a1 1 0 0 1 1-1h2v-2a1 1 0 0 1 2 0v2h2a1 1 0 0 1 0 2h-2v2a1 1 0 0 1-2 0v-2h-2a1 1 0 0 1-1-1z"/></svg>
                    </button>
                    <button class="minus" aria-label="zoom out">
                        <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"><path d="M10.035 18.069a7.981 7.981 0 0 0 3.938-1.035l3.332 3.332a2.164 2.164 0 0 0 3.061-3.061l-3.332-3.332a8.032 8.032 0 0 0-12.68-9.619 8.034 8.034 0 0 0 5.681 13.715zM5.768 5.768A6.033 6.033 0 1 1 4 10.035a5.989 5.989 0 0 1 1.768-4.267zm.267 4.267a1 1 0 0 1 1-1h6a1 1 0 0 1 0 2h-6a1 1 0 0 1-1-1z"/></svg>
                    </button>
                </div>
                <div class="helpers">
                   <!-- <div class="helper">
                    </div> -->
                </div>
            </div>
        `;
        const style = document.createElement('style');
        style.textContent = Controls.styles;
        this.renderRoot.appendChild(style);
    }

    static get styles() {
        return `
            .controls {
                position: absolute;
                top: 0;
                left:0;
                width: 100%;
                height: 90vh;
                pointer-events: none;
            }
            .zoom-control {
                position: absolute;
                top: 10px;
                right: 10px;
                align-self: self-end;
                background: transparent;
                height: 30px;
                display: flex;
                flex-direction: row;
                justify-items: space-between;
                align-items: center;
                pointer-events: auto;
            }
            .zoom-control button {
                display:flex;
                background: #00000088;
                border: none;
                width: fit-content;
                height: inherit;
                align-items: center;
                justify-content: center;
            }
            .zoom-control button svg {
                width: 16px;
                height: 16px;
                fill: #fafafa88;
                stroke: transparent;
            }
           
            .zoom-control button:nth-child(1){
                border-right: 2px solid #000;
                border-radius: 10px 0 0 10px;
            }
            .zoom-control button:nth-child(2){
                border-radius: 0 10px 10px 0;
            }
           
            .zoom-control button:nth-child(2):active{
                border-radius: 0 10px 10px 0;
            }
                
            .zoom-control button:active {
                cursor: pointer;
                background-color: #00000044;
            }
            .zoom-control button:active svg {
                fill: black;
            }

            .zoom-control button:hover{
                cursor: pointer;
            }

            .helpers {
                position: absolute;
                bottom: 10px;
                right: 10px;
                align-self: self-end;
                display: flex;
                flex-direction: row;
                justify-content: space-between;
                align-items: center;
                height: 50px;
                pointer-events: auto;
            }

            .helper {
                border: 1px solid #00000088;
                border-radius: 8px;
                display: flex;
                color: #00000088;
                width: 110px;
                height: fit-content;
                padding: 5px;
                justify-content: space-between;
                flex-direction: row;
                align-items: center;
                font-size: 0.8rem;
                font-weight: bold;
            }
        `;
    }

    connectedCallback() {
        
    }

    /**
     * @type {HTMLDivElement}
     */
    get container() {
        return this.renderRoot.querySelector('.controls');
    }

    /**
     * @type {HTMLDivElement}
     */
    get plus() {
        return this.renderRoot.querySelector('.plus');
    }

    /**
     * @type {HTMLDivElement}
     */
    get minus() {
        return this.renderRoot.querySelector('.minus');
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

customElements.define(`${Namespace}-controls`, Controls);