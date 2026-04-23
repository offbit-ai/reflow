package ai.offbit.reflow;

import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

public class GraphTest {
    @Test
    void authoringYieldsExpectedJson() {
        try (var g = new Graph("demo", false)) {
            g.addNode("a", "tpl_x");
            g.addNode("b", "tpl_y");
            g.addConnection("a", "out", "b", "in");
            g.addInitial("a", "in", "{\"type\":\"Flow\"}");

            String json = g.toJson();
            assertTrue(json.contains("tpl_x"), "missing tpl_x: " + json);
            assertTrue(json.contains("tpl_y"));
        }
    }
}
