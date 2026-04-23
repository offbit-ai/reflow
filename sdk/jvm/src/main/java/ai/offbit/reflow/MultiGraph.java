package ai.offbit.reflow;

/**
 * Multi-graph composition utilities. Merges N {@code GraphExport}
 * documents into a single {@code GraphExport}, namespacing collisions
 * and wiring cross-graph connections.
 *
 * <p>Request shape (JSON):
 *
 * <pre>{@code
 * {
 *   "graphs": [<GraphExport>, ...],
 *   "connections": [{ "from": { "process": "ns/proc", "port": "out" },
 *                     "to":   { "process": "ns/proc", "port": "in" } }],
 *   "shared_resources": [{ "name": "...", "component": "..." }],
 *   "properties": { "name": "composed" },
 *   "case_sensitive": false
 * }
 * }</pre>
 *
 * <p>Returns the composed {@code GraphExport} JSON. Feed it to
 * {@link Graph#fromJson(String)} to load into a {@link Network}.
 */
public final class MultiGraph {
    static { Reflow.ensureLoaded(); }

    private MultiGraph() {}

    /** Compose graphs described by the JSON request. */
    public static String compose(String compositionJson) {
        String out = nativeCompose(compositionJson);
        if (out == null) {
            throw new RuntimeException("compose_graphs returned null");
        }
        return out;
    }

    private static native String nativeCompose(String compositionJson);
}
