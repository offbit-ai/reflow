import "./toolbar.js";
import "./drawer.js";
import { Namespace } from "./util.js";

class App extends HTMLElement {

    constructor() {
        super();

        const shadow = this.attachShadow({ mode: 'closed' });

        shadow.innerHTML = `
            <div class="container">
                <div class="center_view">
                    <div class="stage"></div>
                    <${Namespace}-toolbar></${Namespace}-toolbar> 
                </div>
                <${Namespace}-drawer></${Namespace}-drawer>
            </div>
       `;
        const style = document.createElement("style");
        style.textContent = App.styles;
        shadow.appendChild(style);
    }

    static get styles() {
        return `
            .container {
                zoom:1;
                display: flex;
                justify-content: center;
                flex-direction: row;
                overflow: hidden;
                flex-shrink:1;
                width: 100%;
                background: #22222288
            }

            .center_view {
                zoom:1;
                display: flex;
                flex-direction: column;
                width: 100%;
                height: 100vh;
                flex-shrink:1;
                flex-grow: 1;
                overflow: hidden;
            }

            .stage {
                display: block;
                position: relative;
                width: 100%;
                height:100%;
                flex-shrink:1;
            }
        `;
    };

    connectedCallback() {

    }
}

customElements.define(`${Namespace}-app`, App);

(() => {
    document.getElementById("root").appendChild(new App());
})();