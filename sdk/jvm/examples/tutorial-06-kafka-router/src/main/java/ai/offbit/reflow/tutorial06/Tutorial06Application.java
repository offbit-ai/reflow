// Long-running stream router — the antipode of tutorial 05's per-
// request graph. One Reflow Network boots at startup, runs forever,
// and the Kafka source actor's poll loop is what drives ticks
// through the graph.
//
//   OrderSource ──► Router ──┬─► confirmed ─► orders.confirmed
//                            │              └─► Logger
//                            ├─► cancelled ─► orders.cancelled
//                            │              └─► Logger
//                            ├─► refunded  ─► orders.refunded
//                            │              └─► Logger
//                            └─► other     ─► orders.dlq
//                                            └─► Logger
//
// Each router outport fans into BOTH a KafkaSink AND a Logger —
// Reflow connectors are broadcast, so adding the logger doesn't
// take packets away from the sinks. Routing policy is one method
// (Router.run); changing it doesn't touch the wiring.

package ai.offbit.reflow.tutorial06;

import ai.offbit.reflow.Network;
import ai.offbit.reflow.tutorial06.actors.KafkaSink;
import ai.offbit.reflow.tutorial06.actors.Logger;
import ai.offbit.reflow.tutorial06.actors.OrderSource;
import ai.offbit.reflow.tutorial06.actors.Router;

public class Tutorial06Application {

    public static void main(String[] args) throws Exception {
        String bootstrap = System.getenv().getOrDefault("KAFKA_BOOTSTRAP", "localhost:9092");
        String inputTopic = System.getenv().getOrDefault("INPUT_TOPIC", "orders");
        String groupId    = System.getenv().getOrDefault("GROUP_ID", "reflow-tut06");

        var source       = new OrderSource(bootstrap, inputTopic, groupId);
        var confirmed    = new KafkaSink(bootstrap, "orders.confirmed");
        var cancelled    = new KafkaSink(bootstrap, "orders.cancelled");
        var refunded     = new KafkaSink(bootstrap, "orders.refunded");
        var dlq          = new KafkaSink(bootstrap, "orders.dlq");

        Network net = new Network();
        Runtime.getRuntime().addShutdownHook(new Thread(() -> {
            System.out.println("shutting down…");
            source.stop();
            net.shutdown();
            confirmed.close(); cancelled.close(); refunded.close(); dlq.close();
            net.close();
        }));

        net.registerActor("tpl_source",    source);
        net.registerActor("tpl_router",    new Router());
        net.registerActor("tpl_sink_conf", confirmed);
        net.registerActor("tpl_sink_canc", cancelled);
        net.registerActor("tpl_sink_ref",  refunded);
        net.registerActor("tpl_sink_dlq",  dlq);
        net.registerActor("tpl_log_conf",  new Logger("confirmed"));
        net.registerActor("tpl_log_canc",  new Logger("cancelled"));
        net.registerActor("tpl_log_ref",   new Logger("refunded"));
        net.registerActor("tpl_log_dlq",   new Logger("dlq"));

        net.addNode("source",    "tpl_source");
        net.addNode("router",    "tpl_router");
        net.addNode("confirmed", "tpl_sink_conf");
        net.addNode("cancelled", "tpl_sink_canc");
        net.addNode("refunded",  "tpl_sink_ref");
        net.addNode("dlq",       "tpl_sink_dlq");
        net.addNode("log_conf",  "tpl_log_conf");
        net.addNode("log_canc",  "tpl_log_canc");
        net.addNode("log_ref",   "tpl_log_ref");
        net.addNode("log_dlq",   "tpl_log_dlq");

        net.addConnection("source",  "order",     "router",    "order");
        net.addConnection("router",  "confirmed", "confirmed", "in");
        net.addConnection("router",  "cancelled", "cancelled", "in");
        net.addConnection("router",  "refunded",  "refunded",  "in");
        net.addConnection("router",  "other",     "dlq",       "in");
        // Same router outports fan to the loggers in parallel — the
        // broadcast model means adding these doesn't reduce traffic
        // to the sinks.
        net.addConnection("router",  "confirmed", "log_conf",  "in");
        net.addConnection("router",  "cancelled", "log_canc",  "in");
        net.addConnection("router",  "refunded",  "log_ref",   "in");
        net.addConnection("router",  "other",     "log_dlq",   "in");

        // Source has no upstream; kick it with a Flow initial so
        // the runtime schedules its first run().
        net.addInitial("source", "_trigger", "{\"type\":\"Flow\"}");

        System.out.printf("router started: %s -> {confirmed, cancelled, refunded, dlq}%n", inputTopic);
        net.start();

        // Block forever — shutdown hook handles termination.
        Thread.currentThread().join();
    }
}
