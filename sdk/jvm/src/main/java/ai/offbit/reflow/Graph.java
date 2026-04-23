package ai.offbit.reflow;

/**
 * Reflow {@code Graph} — authorable or loadable from a {@code GraphExport}
 * JSON document (what visual editors emit).
 */
public final class Graph implements AutoCloseable {
    static { Reflow.ensureLoaded(); }

    long nativePtr;

    public Graph() { this("", false); }
    public Graph(String name, boolean caseSensitive) {
        this.nativePtr = nativeNew(name == null ? "" : name, caseSensitive);
    }

    Graph(long ptr) { this.nativePtr = ptr; }

    public static Graph fromJson(String json) {
        return new Graph(nativeFromJson(json));
    }

    public String toJson() {
        return nativeToJson(nativePtr);
    }

    public Graph addNode(String id, String component) {
        nativeAddNode(nativePtr, id, component, null);
        return this;
    }

    public Graph addNode(String id, String component, String metadataJson) {
        nativeAddNode(nativePtr, id, component, metadataJson);
        return this;
    }

    public Graph removeNode(String id) {
        nativeRemoveNode(nativePtr, id);
        return this;
    }

    public Graph addConnection(String outNode, String outPort, String inNode, String inPort) {
        nativeAddConnection(nativePtr, outNode, outPort, inNode, inPort, null);
        return this;
    }

    public Graph addInitial(String node, String port, String dataJson) {
        nativeAddInitial(nativePtr, node, port, dataJson, null);
        return this;
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

    private static native long nativeNew(String name, boolean caseSensitive);
    private static native long nativeFromJson(String json);
    private static native String nativeToJson(long ptr);
    private static native void nativeAddNode(long ptr, String id, String component, String metadataJson);
    private static native void nativeRemoveNode(long ptr, String id);
    private static native void nativeAddConnection(long ptr, String outNode, String outPort, String inNode, String inPort, String metadataJson);
    private static native void nativeAddInitial(long ptr, String node, String port, String dataJson, String metadataJson);
    private static native void nativeFree(long ptr);
}
