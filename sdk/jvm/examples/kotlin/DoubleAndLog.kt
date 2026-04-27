// Idiomatic Kotlin example for the Reflow JVM SDK. Uses the actor { }
// DSL, property-style list literals, and coroutine-based events.
//
// Build / run inside sdk/jvm:
//
//   gradle build
//   java -cp "build/libs/*:..." -Dreflow.native.lib=... DoubleAndLogKt

package ai.offbit.reflow.examples.kotlin

import ai.offbit.reflow.*
import kotlinx.coroutines.runBlocking

fun main() = runBlocking {
    val doubler = actor {
        component = "doubler"
        inports = listOf("in")
        outports = listOf("out")
        onRun { ctx ->
            val n = ctx.inputDataJson("in")?.toLong() ?: 0L
            ctx.emit("out", Message.integer(n * 2))
            ctx.done()
        }
    }

    val log = actor {
        component = "log"
        inports = listOf("in")
        onRun { ctx ->
            println("got: ${ctx.inputDataJson("in")}")
            ctx.done()
        }
    }

    network {
        registerActor("tpl_doubler", doubler)
        registerActor("tpl_log", log)
        addNode("a", "tpl_doubler")
        addNode("b", "tpl_log")
        addConnection("a", "out", "b", "in")
        addInitial("a", "in", """{"type":"Integer","data":21}""")
        start()
        Thread.sleep(300)
        shutdown()
        close()
    }
}
