// Joins three independent enrichments into a single response. Sets
// awaitAllInports = true so its run() fires once per request when
// every parallel branch has produced its packet — Reflow's
// equivalent of a barrier or CompletableFuture.allOf.
//
// Holds a CompletableFuture<String> that the controller awaits;
// completing it ends the per-request graph's lifetime.

package ai.offbit.reflow.tutorial05.actors;

import ai.offbit.reflow.Actor;
import ai.offbit.reflow.ActorCallContext;
import java.util.List;
import java.util.concurrent.CompletableFuture;

public class Merger extends Actor {
    private final CompletableFuture<String> done;

    public Merger(CompletableFuture<String> done) {
        this.done = done;
    }

    @Override public String component() { return "merger"; }
    @Override public List<String> inports()  { return List.of("inventory", "price", "reviews"); }
    @Override public List<String> outports() { return List.of(); }
    @Override public boolean awaitAllInports() { return true; }

    @Override public void run(ActorCallContext ctx) {
        String inv     = ctx.inputDataJson("inventory");
        String price   = ctx.inputDataJson("price");
        String reviews = ctx.inputDataJson("reviews");
        String merged = String.format(
            "{\"inventory\":%s,\"price\":%s,\"reviews\":%s}",
            inv, price, reviews);
        done.complete(merged);
        ctx.done();
    }
}
