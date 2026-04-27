package ai.offbit.reflow;

import org.junit.jupiter.api.Test;
import static org.junit.jupiter.api.Assertions.*;

public class MessageTest {
    @Test
    void constructorsProduceExpectedKind() {
        try (var m = Message.flow()) {
            assertEquals("Flow", m.kind());
        }
        try (var m = Message.integer(42)) {
            assertEquals("Integer", m.kind());
            assertEquals(42L, m.asInteger());
        }
        try (var m = Message.string("hello")) {
            assertEquals("String", m.kind());
            assertEquals("hello", m.asString());
        }
        try (var m = Message.bytes(new byte[]{1, 2, 3})) {
            assertEquals("Bytes", m.kind());
            assertArrayEquals(new byte[]{1, 2, 3}, m.asBytes());
        }
    }

    @Test
    void asJsonRoundTripsThroughFromJson() {
        try (var m = Message.integer(11)) {
            String raw = m.asJson();
            try (var back = Message.fromJson(raw)) {
                assertEquals(11L, back.asInteger());
            }
        }
    }

    @Test
    void dataJsonUnwrapsEnvelope() {
        // Bare scalars
        try (var m = Message.integer(7)) {
            assertEquals("7", m.dataJson());
        }
        try (var m = Message.string("hi")) {
            assertEquals("\"hi\"", m.dataJson());
        }
        try (var m = Message.booleanOf(true)) {
            assertEquals("true", m.dataJson());
        }
        // Variants without a portable JSON payload
        try (var m = Message.flow()) {
            assertNull(m.dataJson());
        }
        try (var m = Message.bytes(new byte[]{1, 2, 3})) {
            assertNull(m.dataJson());
        }
    }
}
