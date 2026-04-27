// Stand-in for a reviews service.

package ai.offbit.reflow.tutorial05.actors;

import ai.offbit.reflow.Actor;
import ai.offbit.reflow.ActorCallContext;
import ai.offbit.reflow.Message;
import java.util.List;

public class ReviewsActor extends Actor {
    @Override public String component() { return "reviews"; }
    @Override public List<String> inports()  { return List.of("sku"); }
    @Override public List<String> outports() { return List.of("out"); }

    @Override public void run(ActorCallContext ctx) {
        String sku = ctx.inputDataJson("sku");
        sleep(180);
        long count = (long) (InventoryActor.stripped(sku).length() * 3);
        double rating = 3.5 + (InventoryActor.stripped(sku).length() % 3) * 0.4;
        String json = String.format(
            "{\"sku\":%s,\"count\":%d,\"avg\":%.2f}", sku, count, rating);
        ctx.emit("out", Message.fromJson("{\"type\":\"Object\",\"data\":" + json + "}"));
        ctx.done();
    }

    static void sleep(long ms) {
        try { Thread.sleep(ms); } catch (InterruptedException e) { Thread.currentThread().interrupt(); }
    }
}
