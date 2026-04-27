// Stand-in for a remote inventory service. Sleeps to simulate
// network latency, then emits an Object message with the available
// stock count. In production this is where you'd call your real
// inventory API.

package ai.offbit.reflow.tutorial05.actors;

import ai.offbit.reflow.Actor;
import ai.offbit.reflow.ActorCallContext;
import ai.offbit.reflow.Message;
import java.util.List;

public class InventoryActor extends Actor {
    @Override public String component() { return "inventory"; }
    @Override public List<String> inports()  { return List.of("sku"); }
    @Override public List<String> outports() { return List.of("out"); }

    @Override public void run(ActorCallContext ctx) {
        String sku = ctx.inputDataJson("sku");
        sleep(150);
        // Deterministic stub so tests can assert: stock = sku length * 7.
        long stock = (long) (stripped(sku).length() * 7);
        String json = String.format("{\"sku\":%s,\"stock\":%d}", sku, stock);
        ctx.emit("out", Message.fromJson("{\"type\":\"Object\",\"data\":" + json + "}"));
        ctx.done();
    }

    static String stripped(String j) {
        return (j != null && j.length() >= 2 && j.charAt(0) == '"')
            ? j.substring(1, j.length() - 1)
            : j;
    }

    static void sleep(long ms) {
        try { Thread.sleep(ms); } catch (InterruptedException e) { Thread.currentThread().interrupt(); }
    }
}
