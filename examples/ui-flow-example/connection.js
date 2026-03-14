export class Connection {
    constructor(startPort, endX, endY, svg, zoom, offset) {
        this.connectionLine = svg;
        this.element = document.createElementNS(
            "http://www.w3.org/2000/svg",
            "path",
        );

        this.element.setAttribute("fill", "none");
        this.element.setAttribute("stroke", "#222222");
        this.element.setAttribute("stroke-width", "3");
        svg.appendChild(this.element);

        this.startPort = startPort;
        this.endPort = null;
        this.endX = endX;
        this.endY = endY;

        this.startDrag = false;

        this.update(offset);
    }

    setEndPort(endPort, zoom, offset) {
        this.endPort = endPort;
        this.update(zoom, offset);
    }

    update() {
        const startRect = this.startPort.getBoundingClientRect();
        const containerRect = this.connectionLine.getBoundingClientRect();

        const startX = startRect.left + startRect.width / 2 - containerRect.left;
        const startY = startRect.top + startRect.height / 2 - containerRect.top;

        let endX, endY;
        if (this.endPort) {
            const endRect = this.endPort.getBoundingClientRect();
            endX = endRect.left + endRect.width / 2 - containerRect.left;
            endY = endRect.top + endRect.height / 2 - containerRect.top;
        } else if (this.startDrag) {
            endX = this.endX - containerRect.left;
            endY = this.endY - containerRect.top;
        }

        const dx = Math.abs(endX - startX);
        const dy = Math.abs(endY - startY);
        const controlPointOffset = Math.min(dx / 2, 100);

        if (!isNaN(controlPointOffset)) {
            const d = `M ${startX} ${startY}
            C ${startX + controlPointOffset} ${startY},
              ${endX - controlPointOffset} ${endY},
              ${endX} ${endY}`;

            this.element.setAttribute("d", d);
        }
    }

    updateEnd(x, y) {
        this.endX = x
        this.endY = y
        this.update();
    }


    /**
     * @type {{from: {nodeId:string, port:string}, to: {nodeId:string, port:string}} | undefined}
     */
    _graphInfo;
    get graphInfo() {
        return this._graphInfo;
    }
    set graphInfo(info) {
        this.element.setAttribute("data-key", `${info.from.nodeId}-${info.from.port}-to-${info.to.nodeId}-${info.to.port}`);
        return this._graphInfo = info;
    }
}
