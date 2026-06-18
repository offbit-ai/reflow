package ai.offbit.reflow;

/** Subscription on a Network's local trace-event stream. Events are JSON strings. */
public final class TraceStream implements AutoCloseable {
    static { Reflow.ensureLoaded(); }

    private long nativePtr;

    TraceStream(long ptr) { this.nativePtr = ptr; }

    /** Block up to {@code timeoutMs}. Returns the trace-event JSON or null on timeout/close. */
    public String recv(int timeoutMs) {
        return nativeRecv(nativePtr, timeoutMs);
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

    private static native String nativeRecv(long ptr, int timeoutMs);
    private static native void nativeFree(long ptr);
}
