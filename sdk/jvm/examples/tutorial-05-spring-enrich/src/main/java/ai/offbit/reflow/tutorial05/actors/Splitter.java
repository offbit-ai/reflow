// Reads a SKU from its `sku` inport and broadcasts it on three named
// outports. Reflow connectors are broadcast — every connector from a
// source fires for every packet on that source's outport — so to fan
// the same SKU to three different consumers we declare one outport
// per consumer and emit on each.

package ai.offbit.reflow.tutorial05.actors;

import ai.offbit.reflow.Actor;
import ai.offbit.reflow.ActorCallContext;
import ai.offbit.reflow.Message;
import java.util.List;

public class Splitter extends Actor {
    @Override public String component() { return "splitter"; }
    @Override public List<String> inports()  { return List.of("sku"); }
    @Override public List<String> outports() { return List.of("inv", "price", "reviews"); }

    @Override public void run(ActorCallContext ctx) {
        String sku = stripQuotes(ctx.inputDataJson("sku"));
        ctx.emit("inv",     Message.string(sku));
        ctx.emit("price",   Message.string(sku));
        ctx.emit("reviews", Message.string(sku));
        ctx.done();
    }

    static String stripQuotes(String json) {
        if (json == null || json.length() < 2) return json;
        if (json.charAt(0) == '"' && json.charAt(json.length() - 1) == '"') {
            return json.substring(1, json.length() - 1);
        }
        return json;
    }
}
