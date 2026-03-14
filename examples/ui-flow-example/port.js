import { Namespace } from './util.js';
import { uuidv4 } from './uuid/v4.js';


export const PortViewTrait = Object.freeze({
    OPTION: 'OPTION',
    TEXT_INPUT: 'TEXT_INPUT',
    NUMBER_INPUT: 'NUMBER_INPUT',
    BOOLEAN: 'BOOLEAN',
    PULSE: 'PULSE',
    COLOR_PICKER: 'COLOR_PICKER',
    STANDARD: 'STANDARD'
});

/**
 * @typedef {object} Position
 * @property {number} x 
 * @property {number} y
 */

/**
 * @typedef {object} PortDirection
 * @property {'in'} in
 * @property {'out'} out
 */

/**
 * @typedef {object} PortOptions
 * @property {string} name
 * @property {PortDirection} direction
 * @property {Boolean} hideLabel
 * @property {Position} position
 * @property {string} nodeId
 * @property {PortViewTrait} trait
 * @property {any} viewModel
 */

function GenericViewModel(T, Base) {
    return class extends Base {
        /**
         * @type {HTMLElement | null}
         */
        portView = null;
        field = new T();

        setId(_id) { }

        getData = () => { }
        setData = (v) => { }
        initView = () => { }
    }
}

export class PortViewModel { }

export class BooleanPortViewModel extends GenericViewModel(Boolean, PortViewModel) {
    trait = PortViewTrait.BOOLEAN;

    constructor(defaultValue = false) {
        super();
        this.portView = document.createElement('input');
        this.portView.setAttribute('type', 'checkbox');
        this.portView.checked = defaultValue;
        this.field = defaultValue;
        this.portView.addEventListener('change', this._onUpdate.bind(this));
    }

    initView = () => {
        const style = document.createElement('style');
        style.textContent = `
            input[type="checkbox"] {
                appearance: none;
                margin: 5px;
            }
            input[type="checkbox"]::before {
                display: block;
                content: "";
                width: 0.8rem;
                height: 0.8rem;
                background-color: #ccc;
                padding: 0.02rem;
                border-radius: 2px;
                border: 0.5px solid #22222255;
            }
            input[type="checkbox"]:checked::before {
                background-color: black;
            }
            input[type="checkbox"]:checked::after{
                position: absolute;
                display: inline-block;
                top: inherit;
                left: inherit;
                margin-top: -0.7rem;
                margin-left: 0.33rem;
                content: "";
                width: 0.1rem;
                height: 0.35rem;
                border: solid white;
                border-width: 0 1.5px 1.5px 0;
                -webkit-transform: rotate(45deg);
                -ms-transform: rotate(45deg);
                transform: rotate(45deg);
                padding: 0.02rem;
            }
        `;
        this.portView.appendChild(style);
    }

    setData = (value) => {
        this.portView.checked = value;
    }

    getData = () => {
        return this.portView.checked
    }


    setId(_id) {
        super.setId(_id);
        this.portView.setAttribute('id', _id);
    }

    /**
     * 
     * @param {InputEvent} e 
     */
    _onUpdate(e) {
        this.data = e.target.checked;
        this.onUpdate(e);
    }

    onUpdate = (e) => { }
}

export class PulsePortViewModel extends BooleanPortViewModel {
    trait = PortViewTrait.PULSE;
    constructor() {
        super();
    }

    initView = () => {
        const style = document.createElement('style');
        style.textContent = `
            input[type="checkbox"] {
                appearance: none;
                margin: 5px;
                content: "";
                background-color: #ccc;
                border-radius: 2px;
                width: 0.85rem;
                height: 0.85rem;
                border: 0.5px solid #22222255;
            }
            input[type="checkbox"]::before {
                position: absolute;
                display: inline-block;
                 top: inherit;
                left: inherit;
                content: "";
                margin-top: 0.15rem;
                margin-left: 0.13rem;
                width: 0.5rem;
                height: 0.5rem;
                background-color: #00000044;
                opacity: 0.7;
                padding: 0.02rem;
                border-radius: 0.8rem;
            }
          
            input[type="checkbox"]:checked::after{
                position: absolute;
                display: inline-block;
                top: inherit;
                left: inherit;
                content: "";
                margin-top: 0.18rem;
                margin-left: 0.18rem;
                width: 0.5rem;
                height: 0.5rem;
                border-radius: 0.4rem;
                background-color: #00000044;
                opacity: 0.7;
                animation: pulse 0.5s linear reverse;
            }

            @keyframes pulse {
                from {
                    opacity: 0.5;
                    background-color: #00000044;
                }

                to {
                   opacity: 0.7;
                    background-color: #fff;
                }
            }
        `;
        this.portView.appendChild(style);
    }

    _onUpdate(e) {
        super._onUpdate(e);
        setTimeout(() => {
            this.portView.checked = false;
        }, 800);
    }

}


export class NumberInputPortViewModel extends GenericViewModel(Number, PortViewModel) {
    trait = PortViewTrait.NUMBER_INPUT;
    constructor(defaultValue = 0) {
        super();
        this.portView = document.createElement('input');
        this.portView.type = 'number';
        this.portView.defaultValue = defaultValue;
        this.field = defaultValue;
        this.portView.disabled = false;
        this.portView.readOnly = false;
        this.portView.style.width = this.portView.value.length + 'ch';
        this.portView.addEventListener('keyup', this._onUpdate.bind(this));
    }

    initView = () => {
        const style = document.createElement('style');
        style.textContent = `
            input[type="number"] {
                background-color:transparent;
                height: 0.85rem;
                border:none;
                color: #00000088;
                min-width: 10px;
                font-size: 0.75rem;
                user-select: text;
                text-align: center;
            }
            input[type="number"]:focus{
                outline: 0.5px solid #00000056;
                border-radius: 2px;
            }
            /* Chrome, Safari, Edge, Opera */
            input::-webkit-outer-spin-button,
            input::-webkit-inner-spin-button {
                -webkit-appearance: none;
                margin: 0;
            }

            /* Firefox */
            input[type=number] {
                -moz-appearance: textfield;
            }
            `;
        this.portView.appendChild(style);
    }

    setData = (value) => {
        this.portView.value = value;
        this.portView.style.width = this.portView.value.length + 'ch';
    }

    getData = () => {
        return this.portView.value
    }

    setId(_id) {
        this.portView.setAttribute('id', _id);
    }

    _onUpdate(e) {
        e.preventDefault();
        this.portView.style.width = e.target.value.length + 'ch';
        this.onUpdate(e);
    }

    onUpdate = (e) => { }

}

export class TextInputPortViewModel extends GenericViewModel(String, PortViewModel) {
    trait = PortViewTrait.TEXT_INPUT;
    /**
     * 
     * @param {string | undefined} defaultValue 
     */
    constructor(defaultValue) {
        super();
        this.portView = document.createElement('input');
        this.portView.type = 'text';
        this.portView.defaultValue = defaultValue;
        this.field = defaultValue;
        this.portView.disabled = false;
        this.portView.readOnly = false;
        this.portView.style.width = this.portView.value.length + 'ch';
        this.portView.addEventListener('keyup', this._onUpdate.bind(this));
    }
    initView = () => {
        const style = document.createElement('style');
        style.textContent = `
            input[type="text"] {
                background-color:transparent;
                height: 0.85rem;
                border:none;
                color: #00000088;
                min-width: 10px;
                font-size: 0.75rem;
                user-select: text;
                margin: 5px;
            }
            input[type="number"]:focus{
                outline: 0.5px solid #00000056;
                border-radius: 2px;
            }
            `;
        this.portView.appendChild(style);
    }

    setData = (value) => {
        this.portView.value = value;
        this.portView.style.width = this.portView.value.length + 'ch';
    }

    getData = () => {
        return this.portView.value
    }

    setId(_id) {
        this.portView.setAttribute('id', _id);
    }

    _onUpdate(e) {
        e.preventDefault();
        this.portView.style.width = e.target.value.length + 'ch';
        this.onUpdate(e);
    }

    onUpdate = (e) => { }
}

export class OptionsItem {
    /**
     * @type {string | number}
     */
    id;
    /**
     * @type {string | number}
     */
    displayText;
    /**
     * @type {object | string | number}
     */
    value;
    /**
     * @type {Boolean}
     */
    selected
}

export class OptionsPortViewModel extends GenericViewModel(Array, PortViewModel) {
    trait = PortViewTrait.OPTION;
    /**
     * 
     * @param {Array<OptionsItem>} options 
     */
    constructor(options) {
        super();
        if (options instanceof Array) {
            this.field = options;
            /**
             * @type {HTMLSelectElement}
             */
            this.portView = document.createElement('select');
            this.field.forEach((option) => {
                if (Object.keys(new OptionsItem()).sort().join(",") === Object.keys(option).sort().join(",")) {
                    const op = document.createElement('option');
                    op.value = option.id;
                    op.selected = option.selected;
                    op.dataset["value"] = option.value;
                    op.innerText = option.displayText;
                    this.portView.appendChild(op);
                } else {
                    throw "Option must be type of OptionsItem"
                }
            });
            this.portView.addEventListener('change', this._onUpdate.bind(this));
        } else {
            throw "property must be an array of options"
        }
    }

    initView = () => {

    }

    getData = () => {
        return Array.from(this.portView.options).map((option) => {
           return ({ id: option.value, selected: option.selected, value: option.dataset["value"], displayText: option.innerText });
        });
    }

    setSelectedOption = (id) => {
        this.portView.querySelectorAll('option').forEach((option) => {
            if (option.value === id) {
                option.selected = true
            }
        });
    }

    setId(_id) {
        this.portView.setAttribute('id', _id);
    }

    /**
     * 
     * @param {Event} e 
     */
    _onUpdate(e) {
        // e.preventDefault();
        this.onUpdate(e);
    }
   
    onUpdate = (e) => { }
}

export class Port extends HTMLElement {
    /**
     * @type {string}
     */
    _id
    /**
     * @type {Position}
     */
    _position;

    /**
     * @type {string}
     */
    _name;

    /**
     * @type {PortDirection}
     */
    _direction;

    /**
     * @type {string | undefined}
     */
    _nodeId

    /**
     * 
     * @param {PortOptions} options 
     */
    constructor(options) {
        super();
        this._name = options.name;
        this.position = options.position;
        this._direction = options.direction;
        this._id = options.id ?? uuidv4().replaceAll("-", "");
        this._hideLabel = options.hideLabel ?? false;
        this._nodeId = options.nodeId;

        switch (options.trait) {
            case PortViewTrait.NUMBER_INPUT: {
                const view = new NumberInputPortViewModel(options.viewModel);
                view.setId(`${this._id}-view`);
                view.onUpdate = this.onUpdate.bind(this);
                Object.assign(this, view);
                break;
            }
            case PortViewTrait.TEXT_INPUT: {
                const view = new TextInputPortViewModel(options.viewModel);
                view.setId(`${this._id}-view`);
                view.onUpdate = this.onUpdate.bind(this);
                Object.assign(this, view);
                break;
            }
            case PortViewTrait.BOOLEAN: {
                const view = new BooleanPortViewModel(options.viewModel);
                view.setId(`${this._id}-view`);
                view.onUpdate = this.onUpdate.bind(this);
                Object.assign(this, view);
                break;
            }
            case PortViewTrait.PULSE: {
                const view = new PulsePortViewModel(options.viewModel);
                view.setId(`${this._id}-view`);
                view.onUpdate = this.onUpdate.bind(this);
                Object.assign(this, view);
                break;
            }
            case PortViewTrait.OPTION: {
                const view = new OptionsPortViewModel(options.viewModel);
                view.setId(`${this._id}-view`);
                view.onUpdate = this.onUpdate.bind(this);
                Object.assign(this, view);
                break;
            }
            default: { }
        }

        this.renderRoot = this.attachShadow({ mode: 'open' });
        this.setAttribute("id", this.id);
        this.setAttribute("name", options.name);
        this.setAttribute("trait", options.trait);
        this.renderRoot.innerHTML = `
            <div id=${this.id} data-direction=${this.direction} data-name=${this.name} data-node-id=${this.nodeId} data-trait=${options.trait} class="port-container">
                ${this.direction === 'out' ? `<div class="port-view-wrapper" style="justify-content: flex-end">
                    <label for="${this.id}-view" style="margin-right: 0.25rem" class="port-name">
                        <div class="port-view"></div>
                        ${!this._hideLabel ? this.name : ''}
                    </label>
                </div>`
                : ''}
                <div id=${this.id}-port data-direction=${this.direction} data-name=${this.name} data-node-id=${this.nodeId} class="port-stub"></div>
                ${this.direction === 'in' ? `<div class="port-view-wrapper" style="justify-content: flex-start">
                    <label for="${this.id}-view" style="margin-left: 0.25rem" class="port-name">
                    ${!this._hideLabel ? this.name : ''}
                    <div class="port-view"></div>
                    </label>
                </div>`
                : ''}
            </div>
        `;
        const style = document.createElement('style');
        style.textContent = Port.styles;
        this.renderRoot.appendChild(style);


    }

    /**
     * @readonly
     */
    get id() {
        return this._id;
    }

    get direction() {
        return this._direction;
    }

    set direction(dir) {
        this._direction = dir;
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

    get portRect() {
        return this.portStub.getBoundingClientRect()
    }

    get nodeId() {
        return this._nodeId;
    }

    set nodeId(id) {
        this._nodeId = id;
    }


    static get styles() {
        return `
            .port-container{
                display: flex;
                flex-direction:row;
                justify-content: space-between;
                align-items: center;
            }
            .port-name {
                font-size: 0.7rem;
                display: flex;
                flex-direction: row;
                justify-content: flex-end;
                align-items: center;
                min-width: 10px;
            }
            .port-stub {
                position: relative;
                width: 0.4rem;
                height: 0.4rem;
                background-color: #00000088;
                border-radius: 0.4rem;
                cursor: crosshair;
            }
            .port-view{
                width: fit-content;
                height: 0.8rem;
                margin: 5px;
                pointer-events: auto;
            }
            .port-view-wrapper{
                width: 100%;
                display: flex;
            }
        `
    }

    connectedCallback() {
        if (this.portView && this._portView && this.getAttribute("trait")) {
            this._portView.replaceWith(this.portView);
            this.initView();
        }
        this.firstUpdated();
    }

    get portStub() {
        return this.renderRoot.querySelector('.port-stub')
    }

    get _portView() {
        return this.renderRoot.querySelector('.port-view')
    }

    firstUpdated() {
        this.zoom = 1;
        this.position = { x: 0, y: 0 };
        this.updatePort(this.zoom)
    }
    /**
     * @param {number} zoom 
     */
    updatePort(zoom) {
        const rect = this.portStub.getBoundingClientRect();
        this.position.x = rect.left / zoom + rect.width / 2;
        this.position.y = rect.top / zoom + rect.height / 2;
    }

    /**
     * Listen to port data changes
     * @param {Event} event 
     */
    onUpdate = (event) => {
        if (this.trait) {
            this.updateCallback(this.getData());
        }
    }

    data() {
        if (this.trait) {
            return this.getData();
        }
    }

    updateCallback = (value) => { };

    getMetadata() {
        const id = this.getAttribute("id");
        const name = this.getAttribute("name");
        const trait = this.getAttribute("trait");

        const data = this.trait ? this.data() : undefined;

        return { id, name, trait, field: this.field, value: data };
    }
}

customElements.define(`${Namespace}-port`, Port);