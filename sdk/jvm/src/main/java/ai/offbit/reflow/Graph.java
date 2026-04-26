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

    public Graph addInitial(String node, String port, String dataJson, String metadataJson) {
        nativeAddInitial(nativePtr, node, port, dataJson, metadataJson);
        return this;
    }

    public Graph addConnection(String outNode, String outPort, String inNode, String inPort, String metadataJson) {
        nativeAddConnection(nativePtr, outNode, outPort, inNode, inPort, metadataJson);
        return this;
    }

    // ─── Mutators (renames) ────────────────────────────────────────────────

    public Graph renameNode(String oldId, String newId) {
        nativeRenameNode(nativePtr, oldId, newId);
        return this;
    }

    public Graph renameInport(String oldPort, String newPort) {
        nativeRenameInport(nativePtr, oldPort, newPort);
        return this;
    }

    public Graph renameOutport(String oldPort, String newPort) {
        nativeRenameOutport(nativePtr, oldPort, newPort);
        return this;
    }

    // ─── Mutators (port lifecycle) ─────────────────────────────────────────

    public Graph addInport(String portId, String nodeId, String portKey) {
        nativeAddInport(nativePtr, portId, nodeId, portKey, null, null);
        return this;
    }

    /**
     * @param portTypeJson optional JSON value matching the {@code PortType}
     *                     enum, e.g. {@code "{\"type\":\"flow\"}"}; null
     *                     means {@code Any}.
     */
    public Graph addInport(String portId, String nodeId, String portKey, String portTypeJson, String metadataJson) {
        nativeAddInport(nativePtr, portId, nodeId, portKey, portTypeJson, metadataJson);
        return this;
    }

    public Graph addOutport(String portId, String nodeId, String portKey) {
        nativeAddOutport(nativePtr, portId, nodeId, portKey, null, null);
        return this;
    }

    public Graph addOutport(String portId, String nodeId, String portKey, String portTypeJson, String metadataJson) {
        nativeAddOutport(nativePtr, portId, nodeId, portKey, portTypeJson, metadataJson);
        return this;
    }

    public Graph removeInport(String portId) {
        nativeRemoveInport(nativePtr, portId);
        return this;
    }

    public Graph removeOutport(String portId) {
        nativeRemoveOutport(nativePtr, portId);
        return this;
    }

    // ─── Mutators (groups) ─────────────────────────────────────────────────

    /** {@code nodesJson} must be a JSON array of strings. */
    public Graph addGroup(String groupId, String nodesJson, String metadataJson) {
        nativeAddGroup(nativePtr, groupId, nodesJson, metadataJson);
        return this;
    }

    public Graph removeGroup(String groupId) {
        nativeRemoveGroup(nativePtr, groupId);
        return this;
    }

    public Graph addToGroup(String groupId, String nodeId) {
        nativeAddToGroup(nativePtr, groupId, nodeId);
        return this;
    }

    public Graph removeFromGroup(String groupId, String nodeId) {
        nativeRemoveFromGroup(nativePtr, groupId, nodeId);
        return this;
    }

    // ─── Mutators (connection / initial removal + indexed initials) ───────

    public Graph removeConnection(String outNode, String outPort, String inNode, String inPort) {
        nativeRemoveConnection(nativePtr, outNode, outPort, inNode, inPort);
        return this;
    }

    public Graph removeInitial(String node, String port) {
        nativeRemoveInitial(nativePtr, node, port);
        return this;
    }

    public Graph addInitialIndex(String node, String port, String dataJson, long index, String metadataJson) {
        nativeAddInitialIndex(nativePtr, node, port, dataJson, index, metadataJson);
        return this;
    }

    public Graph addGraphInitial(String inport, String dataJson) {
        nativeAddGraphInitial(nativePtr, inport, dataJson, null);
        return this;
    }

    public Graph addGraphInitial(String inport, String dataJson, String metadataJson) {
        nativeAddGraphInitial(nativePtr, inport, dataJson, metadataJson);
        return this;
    }

    public Graph addGraphInitialIndex(String inport, String dataJson, long index, String metadataJson) {
        nativeAddGraphInitialIndex(nativePtr, inport, dataJson, index, metadataJson);
        return this;
    }

    public Graph removeGraphInitial(String inport) {
        nativeRemoveGraphInitial(nativePtr, inport);
        return this;
    }

    // ─── Mutators (metadata setters + properties) ─────────────────────────

    public Graph setNodeMetadata(String id, String metadataJson) {
        nativeSetNodeMetadata(nativePtr, id, metadataJson);
        return this;
    }

    public Graph setConnectionMetadata(String outNode, String outPort, String inNode, String inPort, String metadataJson) {
        nativeSetConnectionMetadata(nativePtr, outNode, outPort, inNode, inPort, metadataJson);
        return this;
    }

    public Graph setInportMetadata(String portId, String metadataJson) {
        nativeSetInportMetadata(nativePtr, portId, metadataJson);
        return this;
    }

    public Graph setOutportMetadata(String portId, String metadataJson) {
        nativeSetOutportMetadata(nativePtr, portId, metadataJson);
        return this;
    }

    public Graph setGroupMetadata(String groupId, String metadataJson) {
        nativeSetGroupMetadata(nativePtr, groupId, metadataJson);
        return this;
    }

    public Graph setProperties(String propertiesJson) {
        nativeSetProperties(nativePtr, propertiesJson);
        return this;
    }

    /**
     * Replace this graph's contents with another {@code GraphExport}.
     * Destructive — clears existing nodes, connections, properties, etc.
     */
    public Graph importJson(String exportJson) {
        nativeImport(nativePtr, exportJson);
        return this;
    }

    // ─── Queries (return JSON; null means "not found" for getNode/getConnection) ──

    public String getNodeJson(String id) {
        return nativeGetNodeJson(nativePtr, id);
    }

    public String nodesJson() {
        return nativeNodesJson(nativePtr);
    }

    public String getConnectionJson(String outNode, String outPort, String inNode, String inPort) {
        return nativeGetConnectionJson(nativePtr, outNode, outPort, inNode, inPort);
    }

    public String connectionsJson() {
        return nativeConnectionsJson(nativePtr);
    }

    public String groupsJson() {
        return nativeGroupsJson(nativePtr);
    }

    public String inportsJson() {
        return nativeInportsJson(nativePtr);
    }

    public String outportsJson() {
        return nativeOutportsJson(nativePtr);
    }

    public String initializersJson() {
        return nativeInitializersJson(nativePtr);
    }

    public String propertiesJson() {
        return nativePropertiesJson(nativePtr);
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

    // ── native bindings ───────────────────────────────────────────────────
    private static native long nativeNew(String name, boolean caseSensitive);
    private static native long nativeFromJson(String json);
    private static native String nativeToJson(long ptr);
    private static native void nativeAddNode(long ptr, String id, String component, String metadataJson);
    private static native void nativeRemoveNode(long ptr, String id);
    private static native void nativeAddConnection(long ptr, String outNode, String outPort, String inNode, String inPort, String metadataJson);
    private static native void nativeAddInitial(long ptr, String node, String port, String dataJson, String metadataJson);
    private static native void nativeFree(long ptr);

    // renames
    private static native void nativeRenameNode(long ptr, String oldId, String newId);
    private static native void nativeRenameInport(long ptr, String oldPort, String newPort);
    private static native void nativeRenameOutport(long ptr, String oldPort, String newPort);

    // ports
    private static native void nativeAddInport(long ptr, String portId, String nodeId, String portKey, String portTypeJson, String metadataJson);
    private static native void nativeAddOutport(long ptr, String portId, String nodeId, String portKey, String portTypeJson, String metadataJson);
    private static native void nativeRemoveInport(long ptr, String portId);
    private static native void nativeRemoveOutport(long ptr, String portId);

    // groups
    private static native void nativeAddGroup(long ptr, String groupId, String nodesJson, String metadataJson);
    private static native void nativeRemoveGroup(long ptr, String groupId);
    private static native void nativeAddToGroup(long ptr, String groupId, String nodeId);
    private static native void nativeRemoveFromGroup(long ptr, String groupId, String nodeId);

    // removals + indexed initials
    private static native void nativeRemoveConnection(long ptr, String outNode, String outPort, String inNode, String inPort);
    private static native void nativeRemoveInitial(long ptr, String node, String port);
    private static native void nativeAddInitialIndex(long ptr, String node, String port, String dataJson, long index, String metadataJson);
    private static native void nativeAddGraphInitial(long ptr, String inport, String dataJson, String metadataJson);
    private static native void nativeAddGraphInitialIndex(long ptr, String inport, String dataJson, long index, String metadataJson);
    private static native void nativeRemoveGraphInitial(long ptr, String inport);

    // metadata setters + properties
    private static native void nativeSetNodeMetadata(long ptr, String id, String metadataJson);
    private static native void nativeSetConnectionMetadata(long ptr, String outNode, String outPort, String inNode, String inPort, String metadataJson);
    private static native void nativeSetInportMetadata(long ptr, String portId, String metadataJson);
    private static native void nativeSetOutportMetadata(long ptr, String portId, String metadataJson);
    private static native void nativeSetGroupMetadata(long ptr, String groupId, String metadataJson);
    private static native void nativeSetProperties(long ptr, String propertiesJson);
    private static native void nativeImport(long ptr, String exportJson);

    // queries
    private static native String nativeGetNodeJson(long ptr, String id);
    private static native String nativeNodesJson(long ptr);
    private static native String nativeGetConnectionJson(long ptr, String outNode, String outPort, String inNode, String inPort);
    private static native String nativeConnectionsJson(long ptr);
    private static native String nativeGroupsJson(long ptr);
    private static native String nativeInportsJson(long ptr);
    private static native String nativeOutportsJson(long ptr);
    private static native String nativeInitializersJson(long ptr);
    private static native String nativePropertiesJson(long ptr);
}
