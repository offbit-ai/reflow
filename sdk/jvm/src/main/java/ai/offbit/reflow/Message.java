package ai.offbit.reflow;

/**
 * A Reflow {@code Message}. Construct with static factories; inspect
 * with {@link #kind()} and the typed {@code asXxx} accessors.
 *
 * <p>Message instances own a native handle. They are freed automatically
 * on garbage collection, but emitting via
 * {@link ActorCallContext#emit(String, Message)} transfers ownership to
 * the runtime — do not reuse an emitted instance.
 */
public final class Message implements AutoCloseable {
    static { Reflow.ensureLoaded(); }

    long nativePtr;

    Message(long ptr) {
        this.nativePtr = ptr;
    }

    public static Message flow() { return new Message(nativeFlow()); }
    public static Message booleanOf(boolean v) { return new Message(nativeBoolean(v)); }
    public static Message integer(long v) { return new Message(nativeInteger(v)); }
    public static Message floatOf(double v) { return new Message(nativeFloat(v)); }
    public static Message string(String v) { return new Message(nativeString(v)); }
    public static Message bytes(byte[] v) { return new Message(nativeBytes(v)); }
    public static Message error(String v) { return new Message(nativeError(v)); }

    /** Parse a tagged {@code Message} JSON (e.g. {@code {"type":"Integer","data":3}}). */
    public static Message fromJson(String json) { return new Message(nativeFromJson(json)); }

    public String kind() {
        if (nativePtr == 0) return "Other";
        return nativeKind(nativePtr);
    }

    public String asJson() {
        if (nativePtr == 0) return null;
        return nativeAsJson(nativePtr);
    }

    public String asString() {
        if (nativePtr == 0) return null;
        return nativeAsString(nativePtr);
    }

    /** Returns the integer value, or throws if the message is not an Integer. */
    public long asInteger() {
        if (!"Integer".equals(kind())) {
            throw new IllegalStateException("message kind is " + kind() + ", not Integer");
        }
        return nativeAsInteger(nativePtr);
    }

    public double asFloat() {
        if (!"Float".equals(kind())) {
            throw new IllegalStateException("message kind is " + kind() + ", not Float");
        }
        return nativeAsFloat(nativePtr);
    }

    public boolean asBoolean() {
        if (!"Boolean".equals(kind())) {
            throw new IllegalStateException("message kind is " + kind() + ", not Boolean");
        }
        return nativeAsBoolean(nativePtr);
    }

    public byte[] asBytes() {
        if (nativePtr == 0) return null;
        return nativeAsBytes(nativePtr);
    }

    /** For {@code StreamHandle} messages, claim the consumer-side reader. */
    public StreamReader takeStream() {
        if (nativePtr == 0) throw new IllegalStateException("message is closed");
        long rx = nativeTakeStream(nativePtr);
        if (rx == 0) throw new IllegalStateException("not a StreamHandle or receiver gone");
        return new StreamReader(rx);
    }

    @Override
    public void close() {
        if (nativePtr != 0) {
            nativeFree(nativePtr);
            nativePtr = 0;
        }
    }

    @Override
    @SuppressWarnings("deprecation")
    protected void finalize() {
        close();
    }

    // ─── native bindings ──────────────────────────────────────────────────

    private static native long nativeFlow();
    private static native long nativeBoolean(boolean v);
    private static native long nativeInteger(long v);
    private static native long nativeFloat(double v);
    private static native long nativeString(String v);
    private static native long nativeBytes(byte[] v);
    private static native long nativeError(String v);
    private static native long nativeFromJson(String json);
    private static native String nativeKind(long ptr);
    private static native String nativeAsJson(long ptr);
    private static native String nativeAsString(long ptr);
    private static native long nativeAsInteger(long ptr);
    private static native double nativeAsFloat(long ptr);
    private static native boolean nativeAsBoolean(long ptr);
    private static native byte[] nativeAsBytes(long ptr);
    private static native long nativeTakeStream(long ptr);
    private static native void nativeFree(long ptr);
}
