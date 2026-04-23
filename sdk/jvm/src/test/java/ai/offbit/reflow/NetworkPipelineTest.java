package ai.offbit.reflow;

import java.util.List;
import java.util.concurrent.ArrayBlockingQueue;
import java.util.concurrent.BlockingQueue;
import java.util.concurrent.TimeUnit;

import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

public class NetworkPipelineTest {

    static class Doubler extends Actor {
        @Override public String component() { return "doubler"; }
        @Override public List<String> inports() { return List.of("in"); }
        @Override public List<String> outports() { return List.of("out"); }

        @Override public void run(ActorCallContext ctx) {
            // Parse the inputs JSON to find the integer value arriving on "in".
            // Shape: {"in": {"type": "Integer", "data": 21}}
            String raw = ctx.inputsJson();
            long n = parseIntegerInput(raw, "in");
            ctx.emit("out", Message.integer(n * 2));
            ctx.done();
        }

        private static long parseIntegerInput(String json, String port) {
            // Minimal manual parse — tests don't pull in a JSON dep.
            String key = "\"" + port + "\"";
            int i = json.indexOf(key);
            if (i < 0) return 0;
            int d = json.indexOf("\"data\"", i);
            if (d < 0) return 0;
            int colon = json.indexOf(':', d);
            int end = indexOfAny(json, colon + 1, new char[]{',', '}'});
            String num = json.substring(colon + 1, end).trim();
            return Long.parseLong(num);
        }

        private static int indexOfAny(String s, int from, char[] chars) {
            for (int i = from; i < s.length(); i++) {
                for (char c : chars) if (s.charAt(i) == c) return i;
            }
            return s.length();
        }
    }

    static class Collect extends Actor {
        final BlockingQueue<Long> got = new ArrayBlockingQueue<>(4);
        @Override public String component() { return "collect"; }
        @Override public List<String> inports() { return List.of("in"); }
        @Override public void run(ActorCallContext ctx) {
            String raw = ctx.inputsJson();
            long v = Doubler.parseIntegerInput(raw, "in");
            got.offer(v);
            ctx.done();
        }
    }

    @Test
    void endToEndPipelineDoubles() throws Exception {
        Collect c = new Collect();
        try (var net = new Network()) {
            net.registerActor("tpl_doubler", new Doubler());
            net.registerActor("tpl_collect", c);

            net.addNode("d", "tpl_doubler");
            net.addNode("co", "tpl_collect");
            net.addConnection("d", "out", "co", "in");
            net.addInitial("d", "in", "{\"type\":\"Integer\",\"data\":21}");

            net.start();
            Long got = c.got.poll(2, TimeUnit.SECONDS);
            net.shutdown();

            assertNotNull(got, "timed out waiting for collector");
            assertEquals(42L, (long) got);
        }
    }
}
