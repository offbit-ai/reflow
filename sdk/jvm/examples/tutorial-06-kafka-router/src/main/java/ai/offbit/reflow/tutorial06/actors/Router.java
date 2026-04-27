// Inspects the order's status field and emits on the matching
// outport. Routing policy is one method — swap status-based for
// hash-by-customer, geo-by-region, etc., by editing route().
//
// Reflow connectors are broadcast: every connector from a source
// fires for every packet on the source's outport. To *route* (one
// in, exactly one out) we declare distinct outports per
// destination and emit on exactly one per tick.

package ai.offbit.reflow.tutorial06.actors;

import ai.offbit.reflow.Actor;
import ai.offbit.reflow.ActorCallContext;
import ai.offbit.reflow.Message;
import java.util.List;

public class Router extends Actor {
    @Override public String component() { return "router"; }
    @Override public List<String> inports()  { return List.of("order"); }
    @Override public List<String> outports() {
        return List.of("confirmed", "cancelled", "refunded", "other");
    }

    @Override public void run(ActorCallContext ctx) {
        String json = ctx.inputDataJson("order");
        // Bare scalar — strip surrounding quotes to get the raw JSON.
        String body = stripQuotes(json);
        String status = extractStatus(body);
        String port = switch (status) {
            case "confirmed" -> "confirmed";
            case "cancelled" -> "cancelled";
            case "refunded"  -> "refunded";
            default          -> "other";
        };
        ctx.emit(port, Message.string(body));
        ctx.done();
    }

    static String stripQuotes(String s) {
        if (s == null || s.length() < 2) return s;
        if (s.charAt(0) == '"' && s.charAt(s.length() - 1) == '"') {
            return s.substring(1, s.length() - 1).replace("\\\"", "\"");
        }
        return s;
    }

    static String extractStatus(String body) {
        int i = body.indexOf("\"status\"");
        if (i < 0) return "";
        int colon = body.indexOf(':', i);
        if (colon < 0) return "";
        int q1 = body.indexOf('"', colon + 1);
        if (q1 < 0) return "";
        int q2 = body.indexOf('"', q1 + 1);
        if (q2 < 0) return "";
        return body.substring(q1 + 1, q2);
    }
}
