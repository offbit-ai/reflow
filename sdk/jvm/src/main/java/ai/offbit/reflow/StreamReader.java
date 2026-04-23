package ai.offbit.reflow;

/** Consumer-side reader for a {@code Message.StreamHandle}. */
public final class StreamReader implements AutoCloseable {
    static { Reflow.ensureLoaded(); }

    private long nativePtr;

    StreamReader(long ptr) { this.nativePtr = ptr; }

    public enum FrameKind {
        BEGIN, DATA, END, ERROR, TIMEOUT, CLOSED
    }

    public static final class Frame {
        public final FrameKind kind;
        public final byte[] data;
        public final String error;
        Frame(FrameKind kind, byte[] data, String error) {
            this.kind = kind; this.data = data; this.error = error;
        }
    }

    /** Block up to {@code timeoutMs} for a frame. */
    public Frame recv(int timeoutMs) {
        byte[] raw = nativeRecv(nativePtr, timeoutMs);
        if (raw == null || raw.length == 0) {
            return new Frame(FrameKind.TIMEOUT, null, null);
        }
        byte tag = raw[0];
        byte[] rest = new byte[raw.length - 1];
        System.arraycopy(raw, 1, rest, 0, rest.length);
        switch (tag) {
            case 0: return new Frame(FrameKind.BEGIN, null, null);
            case 1: return new Frame(FrameKind.DATA, rest, null);
            case 2: return new Frame(FrameKind.END, null, null);
            case 3: return new Frame(FrameKind.ERROR, null, new String(rest, java.nio.charset.StandardCharsets.UTF_8));
            case 4: return new Frame(FrameKind.TIMEOUT, null, null);
            case 5: return new Frame(FrameKind.CLOSED, null, null);
            default: return new Frame(FrameKind.TIMEOUT, null, null);
        }
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

    private static native byte[] nativeRecv(long ptr, int timeoutMs);
    private static native void nativeFree(long ptr);
}
