package ai.offbit.reflow;

import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

public class GraphFullApiTest {
    @Test
    void renameNodePropagates() {
        try (var g = new Graph("rename", false)) {
            g.addNode("a", "tpl_x").addNode("b", "tpl_y").addConnection("a", "out", "b", "in");
            g.renameNode("a", "alpha");
            String conn = g.getConnectionJson("alpha", "out", "b", "in");
            assertNotNull(conn, "expected connection under alpha->b after rename");
        }
    }

    @Test
    void groupsCrudRoundTrip() {
        try (var g = new Graph("groups", false)) {
            g.addNode("a", "tpl_x").addNode("b", "tpl_y").addNode("c", "tpl_z");
            g.addGroup("g1", "[\"a\",\"b\"]", "{\"tag\":\"left\"}")
             .addToGroup("g1", "c")
             .removeFromGroup("g1", "a")
             .setGroupMetadata("g1", "{\"tag\":\"right\"}");

            String groups = g.groupsJson();
            assertTrue(groups.contains("\"id\":\"g1\""), groups);
            assertTrue(groups.contains("\"b\""), groups);
            assertTrue(groups.contains("\"c\""), groups);
            assertFalse(groups.contains("\"a\""), groups);
            assertTrue(groups.contains("\"tag\":\"right\""), groups);

            g.removeGroup("g1");
            assertEquals("[]", g.groupsJson());
        }
    }

    @Test
    void portsLifecycleAndMetadata() {
        try (var g = new Graph("ports", false)) {
            g.addNode("a", "tpl_x");
            g.addInport("input", "a", "in", "{\"type\":\"flow\"}", null);
            g.addOutport("output", "a", "out", "{\"type\":\"flow\"}", null);

            g.renameInport("input", "left").renameOutport("output", "right");
            g.setInportMetadata("left", "{\"caption\":\"L\"}");
            g.setOutportMetadata("right", "{\"caption\":\"R\"}");

            String inports = g.inportsJson();
            String outports = g.outportsJson();
            assertTrue(inports.contains("\"left\""), inports);
            assertTrue(outports.contains("\"right\""), outports);
            assertTrue(inports.contains("\"caption\":\"L\""), inports);
            assertTrue(outports.contains("\"caption\":\"R\""), outports);

            g.removeInport("left").removeOutport("right");
        }
    }

    @Test
    void connectionAndInitialRemoval() {
        try (var g = new Graph("conn", false)) {
            g.addNode("a", "tpl_x").addNode("b", "tpl_y");
            g.addConnection("a", "out", "b", "in", null);
            g.setConnectionMetadata("a", "out", "b", "in", "{\"weight\":1}");
            g.addInitial("a", "in", "{\"type\":\"Integer\",\"data\":42}");

            assertTrue(g.connectionsJson().contains("\"weight\":1"));
            assertTrue(g.initializersJson().contains("\"Integer\""));

            g.removeConnection("a", "out", "b", "in").removeInitial("a", "in");
            assertEquals("[]", g.connectionsJson());
            assertEquals("[]", g.initializersJson());
        }
    }

    @Test
    void propertiesAndImport() {
        try (var seed = new Graph("seed", false);
             var target = new Graph("target", false)) {
            seed.addNode("x", "tpl_x");
            target.setProperties("{\"author\":\"darmie\"}");
            assertTrue(target.propertiesJson().contains("darmie"));

            target.importJson(seed.toJson());
            assertNotNull(target.getNodeJson("x"));
        }
    }

    @Test
    void getNodeAndQueries() {
        try (var g = new Graph("queries", false)) {
            g.addNode("a", "tpl_x").addNode("b", "tpl_y");
            g.addConnection("a", "out", "b", "in", null);

            String node = g.getNodeJson("a");
            assertNotNull(node);
            assertTrue(node.contains("\"tpl_x\""), node);
            assertNull(g.getNodeJson("nope"));

            String nodes = g.nodesJson();
            assertTrue(nodes.contains("tpl_x") && nodes.contains("tpl_y"), nodes);
        }
    }
}
