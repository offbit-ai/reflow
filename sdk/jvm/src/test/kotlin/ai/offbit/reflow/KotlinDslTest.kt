package ai.offbit.reflow

import kotlinx.coroutines.ExperimentalCoroutinesApi
import kotlinx.coroutines.flow.first
import kotlinx.coroutines.flow.toList
import kotlinx.coroutines.test.runTest
import org.junit.jupiter.api.Test
import java.util.concurrent.ArrayBlockingQueue
import java.util.concurrent.TimeUnit
import kotlin.test.assertEquals
import kotlin.test.assertNotNull
import kotlin.test.assertTrue
import kotlin.test.fail

class KotlinDslTest {

    private fun parseInteger(inputsJson: String, port: String): Long {
        // Minimal parse so the DSL tests don't pull in a JSON dep.
        val key = "\"$port\""
        val start = inputsJson.indexOf(key)
        if (start < 0) return 0
        val dataIdx = inputsJson.indexOf("\"data\"", start)
        if (dataIdx < 0) return 0
        val colon = inputsJson.indexOf(':', dataIdx)
        val end = (colon + 1 until inputsJson.length).firstOrNull {
            inputsJson[it] == ',' || inputsJson[it] == '}'
        } ?: inputsJson.length
        return inputsJson.substring(colon + 1, end).trim().toLong()
    }

    @Test
    fun `actor DSL builds an Actor that runs end-to-end`() {
        val bucket = ArrayBlockingQueue<Long>(4)

        val doubler = actor {
            component = "doubler"
            inports = listOf("in")
            outports = listOf("out")
            onRun { ctx ->
                val n = parseInteger(ctx.inputsJson(), "in")
                ctx.emit("out", Message.integer(n * 2))
                ctx.done()
            }
        }

        val collect = actor {
            component = "collect"
            inports = listOf("in")
            onRun { ctx ->
                bucket.offer(parseInteger(ctx.inputsJson(), "in"))
                ctx.done()
            }
        }

        network {
            registerActor("tpl_doubler", doubler)
            registerActor("tpl_collect", collect)
            addNode("d", "tpl_doubler")
            addNode("c", "tpl_collect")
            addConnection("d", "out", "c", "in")
            addInitial("d", "in", """{"type":"Integer","data":21}""")
            start()

            val got = bucket.poll(2, TimeUnit.SECONDS)
            shutdown()
            close()

            assertNotNull(got, "timed out waiting for collector")
            assertEquals(42L, got)
        }
    }

    @OptIn(ExperimentalCoroutinesApi::class)
    @Test
    fun `EventStream asFlow yields NetworkStarted`() = runTest {
        val nop = actor {
            component = "nop"
            inports = listOf("in")
            onRun { ctx -> ctx.done() }
        }

        val net = Network().apply {
            registerActor("tpl_nop", nop)
            addNode("n", "tpl_nop")
        }

        try {
            val events = net.events()
            net.start()

            // Drain until we see NetworkStarted or time out.
            val deadline = System.currentTimeMillis() + 2000
            var seen = false
            while (System.currentTimeMillis() < deadline) {
                val e = events.recvAsync(150) ?: continue
                if (e.contains("NetworkStarted")) {
                    seen = true
                    break
                }
            }
            events.close()
            assertTrue(seen, "never saw NetworkStarted")
        } finally {
            net.shutdown()
            net.close()
        }
    }

    @Test
    fun `stream flow destructuring works`() {
        Stream.create(16).use { s ->
            s.sendBytes("hello".toByteArray())
            s.end()
            val msg = s.intoMessage()
            try {
                msg.takeStream().use { rx ->
                    val collected = mutableListOf<String>()
                    var ended = false
                    while (!ended) {
                        val (kind, data, error) = rx.recv(250)
                        when (kind) {
                            StreamReader.FrameKind.DATA ->
                                collected += data!!.toString(Charsets.UTF_8)
                            StreamReader.FrameKind.END,
                            StreamReader.FrameKind.CLOSED -> ended = true
                            StreamReader.FrameKind.ERROR -> fail("error frame: $error")
                            StreamReader.FrameKind.TIMEOUT,
                            StreamReader.FrameKind.BEGIN -> Unit
                        }
                    }
                    assertEquals(listOf("hello"), collected)
                }
            } finally {
                msg.close()
            }
        }
    }
}
