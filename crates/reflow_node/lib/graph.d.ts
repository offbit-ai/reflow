export interface GraphNode {
    id: string;
    component: string;
    metadata: Map<string, any> | undefined;
}

export interface GraphEdge {
    portName: string;
    portId: string;
    nodeId: string;
    index: number | undefined;
    expose: boolean;
    data: Value | undefined;
    metadata: Map<string, any> | undefined;
    portType: PortType;
}

export type PortType = { type: "any" } | { type: "flow" } | { type: "event" } | { type: "boolean" } | { type: "integer" } | { type: "float" } | { type: "string" } | { type: "object"; value: string } | { type: "array"; value: PortType } | { type: "encoded" } | { type: "stream" } | { type: "option"; value: PortType };


export type PortType =
  | { type: "flow" }
  | { type: "event" }
  | { type: "boolean" }
  | { type: "integer" }
  | { type: "float" }
  | { type: "string" }
  | { type: "object", value: string }
  | { type: "array", value: PortType }
  | { type: "stream" }
  | { type: "encoded" }
  | { type: "any" }
  | { type: "option", value: PortType };

export interface GraphConnection {
    from: GraphEdge;
    to: GraphEdge;
    metadata: Map<string, any> | undefined;
    data: Value | undefined;
}

export interface GraphIIP {
    to: GraphEdge;
    data: any;
    metadata: Map<string, any> | undefined;
}

export interface GraphGroup {
    id: string;
    nodes: string[];
    metadata: Map<string, any> | undefined;
}
