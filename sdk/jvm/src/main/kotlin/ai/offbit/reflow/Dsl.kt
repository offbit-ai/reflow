@file:JvmName("ReflowDsl")

package ai.offbit.reflow

/**
 * Kotlin builder DSL on top of the Java SDK.
 *
 * Lets you author actors inline without subclassing and wire a network
 * with trailing-lambda ergonomics:
 *
 * ```kotlin
 * val doubler = actor {
 *     component = "doubler"
 *     inports = listOf("in")
 *     outports = listOf("out")
 *     onRun { ctx ->
 *         val n = ctx.inputDataJson("in")?.toLong() ?: 0L
 *         ctx.emit("out", Message.integer(n * 2))
 *         ctx.done()
 *     }
 * }
 *
 * network {
 *     registerActor("tpl_doubler", doubler)
 *     addNode("a", "tpl_doubler")
 *     addInitial("a", "in", """{"type":"Integer","data":21}""")
 *     start()
 * }
 * ```
 *
 * `ctx.inputDataJson(port)` returns the bare JSON payload for that
 * port — a primitive scalar in JSON form, an object literal, or an
 * array literal — so primitive ports parse with one `toLong` /
 * `toDouble` / `removeSurrounding("\"")` call.
 */

@DslMarker
annotation class ReflowDslMarker

@ReflowDslMarker
class ActorBuilder {
    var component: String = "anonymous"
    var inports: List<String> = emptyList()
    var outports: List<String> = emptyList()
    var awaitAllInports: Boolean = false

    private var runFn: ((ActorCallContext) -> Unit)? = null

    /** Register the per-tick body. Must be called before the block returns. */
    fun onRun(body: (ActorCallContext) -> Unit) {
        runFn = body
    }

    internal fun build(): Actor {
        val fn = runFn ?: error("actor { } block must call onRun { }")
        val comp = component
        val ins = inports.toList()
        val outs = outports.toList()
        val await = awaitAllInports
        return object : Actor() {
            override fun component() = comp
            override fun inports() = ins
            override fun outports() = outs
            override fun awaitAllInports() = await
            override fun run(ctx: ActorCallContext) {
                fn(ctx)
            }
        }
    }
}

/** Build an Actor inline without subclassing. */
fun actor(block: ActorBuilder.() -> Unit): Actor =
    ActorBuilder().apply(block).build()

/** Trailing-lambda network builder. Returns the Network so the caller
 *  decides when to close it. */
inline fun network(block: Network.() -> Unit): Network =
    Network().apply(block)

/** Trailing-lambda graph builder. */
inline fun graph(
    name: String = "",
    caseSensitive: Boolean = false,
    block: Graph.() -> Unit = {},
): Graph = Graph(name, caseSensitive).apply(block)
