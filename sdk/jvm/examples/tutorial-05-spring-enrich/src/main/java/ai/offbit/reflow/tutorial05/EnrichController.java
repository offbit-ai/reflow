// Per-request Reflow network behind a Spring REST endpoint. A POST
// to /enrich spins up a fresh graph: Splitter fans the SKU out to
// three slow downstream services, the Merger awaits all three and
// returns a merged JSON payload. Network is closed via
// try-with-resources, so every request gets a clean lifecycle —
// no shared state, no leaks between requests.
//
// Without Reflow you'd write:
//
//   var inv     = CompletableFuture.supplyAsync(() -> inventory(sku));
//   var price   = CompletableFuture.supplyAsync(() -> price(sku));
//   var reviews = CompletableFuture.supplyAsync(() -> reviews(sku));
//   CompletableFuture.allOf(inv, price, reviews).join();
//   return merge(inv.get(), price.get(), reviews.get());
//
// With Reflow the dependency graph is the wiring; awaitAllInports on
// the merger replaces allOf().join().

package ai.offbit.reflow.tutorial05;

import ai.offbit.reflow.Network;
import ai.offbit.reflow.tutorial05.actors.*;
import org.springframework.http.MediaType;
import org.springframework.web.bind.annotation.*;

import java.util.concurrent.CompletableFuture;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.TimeoutException;

@RestController
public class EnrichController {

    public record EnrichRequest(String sku) {}

    @PostMapping(value = "/enrich", consumes = MediaType.APPLICATION_JSON_VALUE,
                                    produces = MediaType.APPLICATION_JSON_VALUE)
    public String enrich(@RequestBody EnrichRequest req) throws Exception {
        var done = new CompletableFuture<String>();

        try (var net = new Network()) {
            net.registerActor("tpl_split",   new Splitter());
            net.registerActor("tpl_inv",     new InventoryActor());
            net.registerActor("tpl_price",   new PriceActor());
            net.registerActor("tpl_reviews", new ReviewsActor());
            net.registerActor("tpl_merge",   new Merger(done));

            net.addNode("split",   "tpl_split");
            net.addNode("inv",     "tpl_inv");
            net.addNode("price",   "tpl_price");
            net.addNode("reviews", "tpl_reviews");
            net.addNode("merge",   "tpl_merge");

            // Splitter fans the SKU to each downstream service on its
            // own outport; the runtime's connector filter ensures
            // each service only sees the packet from the matching
            // outport.
            net.addConnection("split", "inv",     "inv",     "sku");
            net.addConnection("split", "price",   "price",   "sku");
            net.addConnection("split", "reviews", "reviews", "sku");

            // Three branches converge on the merger — distinct
            // inports so awaitAllInports knows when to fire.
            net.addConnection("inv",     "out", "merge", "inventory");
            net.addConnection("price",   "out", "merge", "price");
            net.addConnection("reviews", "out", "merge", "reviews");

            net.addInitial("split", "sku",
                "{\"type\":\"String\",\"data\":\"" + req.sku() + "\"}");
            net.start();

            try {
                return done.get(5, TimeUnit.SECONDS);
            } catch (TimeoutException e) {
                throw new RuntimeException("enrichment timed out for sku=" + req.sku());
            }
        }
    }
}
