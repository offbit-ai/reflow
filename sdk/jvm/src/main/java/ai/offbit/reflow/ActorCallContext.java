package ai.offbit.reflow;

/**
 * Per-tick context passed to an {@link Actor}'s {@code run}. Read
 * {@link #inputsJson()} / {@link #configJson()}, emit zero or more
 * outputs via {@link #emit(String, Message)}, and resolve the tick
 * with exactly one of {@link #done()} / {@link #fail(String)}.
 */
public final class ActorCallContext {
    static { Reflow.ensureLoaded(); }

    private final long nativePtr;
    private boolean resolved = false;

    ActorCallContext(long ptr) {
        this.nativePtr = ptr;
    }

    /** Inputs as a JSON string keyed by port name (values are tagged Messages). */
    public String inputsJson() {
        return nativeInputs(nativePtr);
    }

    /**
     * Bare JSON payload for a single input port — the {@code data} field of
     * the message on {@code port}, with the runtime's EncodableValue
     * wrappers transparently decoded. Returns {@code null} if the port had
     * no message this tick or the variant has no portable JSON form (Flow,
     * Bytes). Lets actor code parse a single port without scanning the full
     * inputs envelope.
     */
    public String inputDataJson(String port) {
        return nativeInputDataJson(nativePtr, port);
    }

    /** Per-node config JSON. */
    public String configJson() {
        return nativeConfig(nativePtr);
    }

    /**
     * Queue an output packet on {@code port}. Ownership of the message
     * transfers to the runtime — do not re-emit or close it afterwards.
     */
    public void emit(String port, Message msg) {
        if (resolved) throw new IllegalStateException("actor context already resolved");
        if (msg == null || msg.nativePtr == 0) {
            throw new IllegalArgumentException("emit: message is null");
        }
        long p = msg.nativePtr;
        msg.nativePtr = 0;
        nativeEmit(nativePtr, port, p);
    }

    /** Resolve the tick. Any packets queued via {@link #emit} are flushed. */
    public void done() {
        if (resolved) return;
        resolved = true;
        nativeDone(nativePtr);
    }

    /** Abort the tick with an error reason. */
    public void fail(String reason) {
        if (resolved) return;
        resolved = true;
        nativeFail(nativePtr, reason);
    }

    private static native String nativeInputs(long ptr);
    private static native String nativeInputDataJson(long ptr, String port);
    private static native String nativeConfig(long ptr);
    private static native void nativeEmit(long ptr, String port, long messagePtr);
    private static native void nativeDone(long ptr);
    private static native void nativeFail(long ptr, String reason);
}
