package ai.offbit.reflow;

/** Subscription on a Network's event stream. Events are JSON strings. */
public final class EventStream implements AutoCloseable {
    static { Reflow.ensureLoaded(); }

    private long nativePtr;

    EventStream(long ptr) { this.nativePtr = ptr; }

    /** Block up to {@code timeoutMs}. Returns the event JSON or null on timeout/close. */
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
