package ai.offbit.reflow;

/** Producer side of a {@code Message.StreamHandle}. */
public final class Stream implements AutoCloseable {
    static { Reflow.ensureLoaded(); }

    long nativePtr;

    private Stream(long ptr) { this.nativePtr = ptr; }

    /** Create a new stream. {@code bufferSize &lt;= 0} means unbounded. */
    public static Stream create(int bufferSize) {
        return new Stream(nativeCreate(bufferSize, "", "", ""));
    }

    public static Stream create(int bufferSize, String originActor, String originPort, String contentType) {
        return new Stream(nativeCreate(
            bufferSize,
            originActor == null ? "" : originActor,
            originPort == null ? "" : originPort,
            contentType == null ? "" : contentType
        ));
    }

    public void sendBytes(byte[] data) {
        nativeSendBytes(nativePtr, data);
    }

    public void end() {
        nativeEnd(nativePtr);
    }

    public void error(String reason) {
        nativeError(nativePtr, reason);
    }

    /**
     * Consume the producer and return a {@code Message.StreamHandle}
     * that can be emitted on an output port.
     */
    public Message intoMessage() {
        long p = nativeIntoMessage(nativePtr);
        nativePtr = 0;
        return new Message(p);
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

    private static native long nativeCreate(int bufferSize, String originActor, String originPort, String contentType);
    private static native void nativeSendBytes(long ptr, byte[] data);
    private static native void nativeEnd(long ptr);
    private static native void nativeError(long ptr, String reason);
    private static native long nativeIntoMessage(long ptr);
    private static native void nativeFree(long ptr);
}
