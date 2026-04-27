// Operational tap. Sits on each router outport in parallel with a
// KafkaSink — Reflow connectors are broadcast, so adding a logger
// doesn't take packets away from the sinks. Useful for watching
// traffic flow at the terminal without scraping Kafka.
//
// The label is set per-instance so we can tell which branch fired:
//
//   confirmed  ── confirmed sink
//              └─ Logger("confirmed") → stdout
//
// Each `confirmed` packet hits BOTH branches.

package ai.offbit.reflow.tutorial06.actors;

import ai.offbit.reflow.Actor;
import ai.offbit.reflow.ActorCallContext;
import java.util.List;

public class Logger extends Actor {
    private final String label;

    public Logger(String label) { this.label = label; }

    @Override public String component() { return "logger_" + label; }
    @Override public List<String> inports()  { return List.of("in"); }
    @Override public List<String> outports() { return List.of(); }

    @Override public void run(ActorCallContext ctx) {
        String body = ctx.inputDataJson("in");
        if (body != null && body.length() >= 2 && body.charAt(0) == '"') {
            body = body.substring(1, body.length() - 1).replace("\\\"", "\"");
        }
        System.out.printf("[%-9s] %s%n", label, body);
        ctx.done();
    }
}
