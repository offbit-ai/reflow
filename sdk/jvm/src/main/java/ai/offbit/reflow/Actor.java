package ai.offbit.reflow;

import java.util.List;

/**
 * Base class for JVM-authored Reflow actors.
 *
 * <p>Override {@link #component()}, {@link #inports()},
 * {@link #outports()}, optionally {@link #awaitAllInports()}, and
 * {@link #run(ActorCallContext)}. Pass instances to
 * {@link Network#registerActor(String, Actor)}.
 *
 * <pre>{@code
 * class Doubler extends Actor {
 *     @Override public String component() { return "doubler"; }
 *     @Override public List<String> inports()  { return List.of("in"); }
 *     @Override public List<String> outports() { return List.of("out"); }
 *     @Override public void run(ActorCallContext ctx) {
 *         // parse inputsJson, emit, call ctx.done()
 *     }
 * }
 * }</pre>
 */
public abstract class Actor {
    static { Reflow.ensureLoaded(); }

    public String component() { return "anonymous"; }
    public List<String> inports() { return List.of(); }
    public List<String> outports() { return List.of(); }
    public boolean awaitAllInports() { return false; }

    /** Per-tick behavior. Must resolve ctx via done() or fail(). */
    public abstract void run(ActorCallContext ctx);

    /** Internal — allocate the Rust-side actor handle. */
    long __build() {
        DispatchBridge dispatcher = new DispatchBridge(this);
        List<String> in = inports();
        List<String> out = outports();
        return nativeRegister(
            component(),
            in.toArray(new String[0]),
            out.toArray(new String[0]),
            awaitAllInports(),
            dispatcher
        );
    }

    /**
     * Called from Rust on every tick. {@code cb} is the
     * {@link DispatchBridge} installed by __build.
     */
    @SuppressWarnings("unused") // called via JNI
    static void __dispatch(Object cb, long ctxPtr) {
        if (!(cb instanceof DispatchBridge bridge)) return;
        bridge.dispatchWithPtr(ctxPtr);
    }

    private static native long nativeRegister(
        String component,
        String[] inports,
        String[] outports,
        boolean awaitAllInports,
        Object dispatcher
    );

    /**
     * Bound callback that wraps the user's Actor.run. Public so the JNI
     * dispatcher can reliably {@code instanceof}-check it across class-
     * loader boundaries.
     */
    public static final class DispatchBridge {
        private final Actor actor;

        public DispatchBridge(Actor actor) {
            this.actor = actor;
        }

        public void dispatchWithPtr(long ctxPtr) {
            ActorCallContext ctx = new ActorCallContext(ctxPtr);
            try {
                actor.run(ctx);
            } catch (Throwable t) {
                try {
                    ctx.fail(t.getClass().getSimpleName() + ": " + t.getMessage());
                } catch (Throwable ignored) {
                    // already resolved
                }
            }
        }
    }

    // package-private — reached from subclasses / tests
    static native void nativeFree(long ptr);
}
