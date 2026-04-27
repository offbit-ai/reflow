// Stand-in for a pricing service. Same shape as InventoryActor —
// sleep to simulate latency, then emit an Object message with the
// current price.

package ai.offbit.reflow.tutorial05.actors;

import ai.offbit.reflow.Actor;
import ai.offbit.reflow.ActorCallContext;
import ai.offbit.reflow.Message;
import java.util.List;

public class PriceActor extends Actor {
    @Override public String component() { return "price"; }
    @Override public List<String> inports()  { return List.of("sku"); }
    @Override public List<String> outports() { return List.of("out"); }

    @Override public void run(ActorCallContext ctx) {
        String sku = ctx.inputDataJson("sku");
        sleep(220);
        double price = 9.99 + InventoryActor.stripped(sku).length() * 0.5;
        String json = String.format("{\"sku\":%s,\"amount\":%.2f,\"currency\":\"USD\"}", sku, price);
        ctx.emit("out", Message.fromJson("{\"type\":\"Object\",\"data\":" + json + "}"));
        ctx.done();
    }

    static void sleep(long ms) {
        try { Thread.sleep(ms); } catch (InterruptedException e) { Thread.currentThread().interrupt(); }
    }
}
