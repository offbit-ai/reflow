package ai.offbit.reflow;

/** Reflow runtime network. */
public final class Network implements AutoCloseable {
    static { Reflow.ensureLoaded(); }

    long nativePtr;

    public Network() {
        this.nativePtr = nativeNew();
    }

    public static Network fromGraph(Graph g) {
        return new Network(nativeFromGraph(g.nativePtr));
    }

    private Network(long ptr) { this.nativePtr = ptr; }

    /**
     * Register an actor template. Accepts either a raw actor handle
     * produced by {@link Templates#templateActor(String)} or an
     * {@link Actor} subclass instance — the latter is built lazily.
     */
    public void registerActor(String templateId, Actor actor) {
        nativeRegisterActor(nativePtr, templateId, actor.__build());
    }

    /** Register a bundled template actor from {@link Templates}. */
    public void registerActor(String templateId, long actorPtr) {
        nativeRegisterActor(nativePtr, templateId, actorPtr);
    }

    public void addNode(String id, String templateId) {
        nativeAddNode(nativePtr, id, templateId, null);
    }

    public void addNode(String id, String templateId, String configJson) {
        nativeAddNode(nativePtr, id, templateId, configJson);
    }

    public void addConnection(String fromActor, String fromPort, String toActor, String toPort) {
        nativeAddConnection(nativePtr, fromActor, fromPort, toActor, toPort);
    }

    public void addInitial(String actor, String port, String messageJson) {
        nativeAddInitial(nativePtr, actor, port, messageJson);
    }

    public void start() { nativeStart(nativePtr); }
    public void shutdown() { nativeShutdown(nativePtr); }

    public EventStream events() {
        return new EventStream(nativeEvents(nativePtr));
    }

    /**
     * Subscribe to this network's live trace events (JSON). Requires tracing to
     * be enabled in the config, e.g. {@code new Network("{\"tracing\":{\"server_url\":\"ws://localhost:8080\",\"enabled\":true}}")}.
     * Events stream locally with no collector required.
     */
    public TraceStream traces() {
        return new TraceStream(nativeTraces(nativePtr));
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

    private static native long nativeNew();
    private static native long nativeFromGraph(long graphPtr);
    private static native void nativeRegisterActor(long ptr, String templateId, long actorPtr);
    private static native void nativeAddNode(long ptr, String id, String templateId, String configJson);
    private static native void nativeAddConnection(long ptr, String fromActor, String fromPort, String toActor, String toPort);
    private static native void nativeAddInitial(long ptr, String actor, String port, String messageJson);
    private static native void nativeStart(long ptr);
    private static native void nativeShutdown(long ptr);
    private static native long nativeEvents(long ptr);
    private static native long nativeTraces(long ptr);
    private static native void nativeFree(long ptr);
}
