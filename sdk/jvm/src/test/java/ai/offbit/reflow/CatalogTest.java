package ai.offbit.reflow;

import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

public class CatalogTest {
    @Test
    void templateListIncludesKnownId() {
        String json = Templates.templateListJson();
        assertTrue(json.contains("tpl_passthrough"), "catalog missing tpl_passthrough");
    }

    @Test
    void templateActorResolvesKnownId() {
        long p = Templates.templateActor("tpl_passthrough");
        assertNotEquals(0L, p);
        // Register + discard — this also exercises registerActor(long) on Network.
        try (var net = new Network()) {
            net.registerActor("tpl_p", p);
        }
    }

    @Test
    void templateActorThrowsOnUnknownId() {
        assertThrows(Exception.class, () -> Templates.templateActor("tpl_bogus_xyz"));
    }
}
