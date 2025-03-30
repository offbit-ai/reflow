import { Network, Graph, initSync} from "../pkg/reflow_network.js";

/**
 * Stateful actor
 */
class HelloActor {
  constructor() {
    this._state = {};
    this.inports = ["in"];
    this.outports = ["out"];
  }

  run(input, send) {
    this._state = { val: "_stat_value" };
    send({ out: "Hello" });
  }

  get state() {
    return this._state;
  }

  set state(value) {
    this._state = value;
  }
}

fetch("../pkg/reflow_network_bg.wasm").then(async (res) => {
  initSync(await res.arrayBuffer());

  let graph = new Graph("myGraph", true);
  graph.subscribe((event) => {
    console.log(event);
  });

  graph.addNode("node_id", "ReadFile");
  graph.removeNode("node_id");

  let network = new Network();

  network.registerActor("test_hello_process", new HelloActor());
  network.registerActor("test_world_process", {
    run: async (input, send) => {
      send({ out: `${input.data} World!` });
    },
    inports: ["in"],
    outports: ["out"],
  });
  network.registerActor("log_process", {
    run: async (input, send) => {
      // console.log("[output]", input.data);
    },
    inports: ["in"],
    outports: [],
  });

  network.addNode("test_hello", "test_hello_process");

  network.addNode("test_world", "test_world_process");
  network.addNode("log", "log_process");

  network.addInitial({
    to: {
      actor: "test_hello",
      port: "in",
      initial_data: { Any: "trigger" },
    },
  });

  network.addConnection({
    from: {
      actor: "test_hello",
      port: "out",
    },
    to: {
      actor: "test_world",
      port: "in",
    },
  });
  network.addConnection({
    from: {
      actor: "test_world",
      port: "out",
    },
    to: {
      actor: "log",
      port: "in",
    },
  });

  // Subscribe to network events
  network.next((event) => {
    if (event._type === "FlowTrace") {
      console.log(
        `%cFLOWTRACE %c(${event.from.actorId}):${event.from.port} ==> %c (${JSON.stringify(event.from.data)}) %c ==> ${event.to.port}:(${event.to.actorId}) `,
        "font-weight: bold; background:green; color:white; padding: 2px; border-top-left-radius: 4px; border-bottom-left-radius: 4px;",
        "font-weight: bold; background:#008B8B; color:white; padding: 2px; border-top-left-radius: 6px; border-bottom-left-radius: 6px;margin-left:-4px",
        "font-weight: bold; background:orange; color:white; padding: 2px; border-top-left-radius: 6px; border-bottom-left-radius: 6px; margin-left:-4px;",
        "font-weight: bold; background:#008B8B; color:white; padding: 2px; border-top-left-radius: 6px; border-bottom-left-radius: 6px; margin-left:-4px; border-top-right-radius: 4px; border-bottom-right-radius: 4px;"
      );
    }
  });

  network.start();

  // Shutdown the network with `network.shutdown();`
});
