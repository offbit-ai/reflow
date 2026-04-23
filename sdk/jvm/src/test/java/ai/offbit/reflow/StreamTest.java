package ai.offbit.reflow;

import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;

import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

public class StreamTest {
    @Test
    void producerConsumerRoundTrip() {
        Stream s = Stream.create(16, "a", "out", "application/octet-stream");
        s.sendBytes(new byte[]{1, 2, 3});
        s.sendBytes(new byte[]{9, 8, 7, 6});
        s.end();

        Message msg = s.intoMessage();
        assertEquals("StreamHandle", msg.kind());

        try (StreamReader rx = msg.takeStream()) {
            List<byte[]> collected = new ArrayList<>();
            boolean sawEnd = false;
            for (int i = 0; i < 20 && !sawEnd; i++) {
                StreamReader.Frame f = rx.recv(250);
                switch (f.kind) {
                    case DATA -> collected.add(f.data);
                    case END, CLOSED -> sawEnd = true;
                    case ERROR -> fail("error frame: " + f.error);
                    case TIMEOUT, BEGIN -> {}
                }
            }
            assertTrue(sawEnd, "never saw end frame");
            assertEquals(2, collected.size());
            assertTrue(Arrays.equals(new byte[]{1, 2, 3}, collected.get(0)));
            assertTrue(Arrays.equals(new byte[]{9, 8, 7, 6}, collected.get(1)));
        }
        msg.close();
    }
}
