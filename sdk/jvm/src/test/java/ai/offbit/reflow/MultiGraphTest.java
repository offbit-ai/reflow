package ai.offbit.reflow;

import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

public class MultiGraphTest {
    private static String export(String name, String nodeId) {
        return "{"
            + "\"caseSensitive\":false,"
            + "\"processes\":{\"" + nodeId + "\":{\"id\":\"" + nodeId + "\",\"component\":\"tpl_passthrough\",\"metadata\":null}},"
            + "\"connections\":[],"
            + "\"inports\":{},\"outports\":{},"
            + "\"properties\":{\"name\":\"" + name + "\"},"
            + "\"groups\":[]"
            + "}";
    }

    @Test
    void composeMergesTwoGraphs() {
        String request = "{"
            + "\"graphs\":[" + export("left", "a") + "," + export("right", "b") + "],"
            + "\"connections\":[],"
            + "\"shared_resources\":[],"
            + "\"properties\":{\"name\":\"composed\"},"
            + "\"case_sensitive\":false"
            + "}";
        String out = MultiGraph.compose(request);
        assertTrue(out.contains("left/a"), "missing left/a in composed: " + out);
        assertTrue(out.contains("right/b"), "missing right/b in composed: " + out);
    }

    @Test
    void composeWiresCrossGraphConnection() {
        String request = "{"
            + "\"graphs\":[" + export("gsrc", "src") + "," + export("gsink", "sink") + "],"
            + "\"connections\":[{"
            + "\"from\":{\"process\":\"gsrc/src\",\"port\":\"out\"},"
            + "\"to\":{\"process\":\"gsink/sink\",\"port\":\"in\"}"
            + "}],"
            + "\"properties\":{\"name\":\"pipeline\"}"
            + "}";
        String out = MultiGraph.compose(request);
        // Composed output must include at least one connection.
        assertTrue(out.contains("\"connections\":["), out);
        assertFalse(out.contains("\"connections\":[]"), "expected non-empty connections: " + out);
    }
}
