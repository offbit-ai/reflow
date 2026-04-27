// Long-running Kafka consumer that publishes one Reflow message per
// inbound record. Source actors with no upstream declare a
// `_trigger` inport, get a Flow initial at network start, and
// never return from run() — the loop polls Kafka indefinitely.
//
// Critical detail: we use ctx.send() instead of ctx.emit().
// ctx.emit() accumulates packets in a HashMap that drains only when
// ctx.done() fires; for a never-returning source that never reaches
// done(), emits would be silently lost. ctx.send() pushes straight
// to the outport channel.
//
// Records arrive as JSON like {"id":..., "status":"confirmed", ...}
// — passed through as a String message; the Router parses status to
// pick the outport.

package ai.offbit.reflow.tutorial06.actors;

import ai.offbit.reflow.Actor;
import ai.offbit.reflow.ActorCallContext;
import ai.offbit.reflow.Message;
import org.apache.kafka.clients.consumer.ConsumerConfig;
import org.apache.kafka.clients.consumer.ConsumerRecord;
import org.apache.kafka.clients.consumer.KafkaConsumer;
import org.apache.kafka.common.serialization.StringDeserializer;

import java.time.Duration;
import java.util.List;
import java.util.Properties;

public class OrderSource extends Actor {
    private final String bootstrap;
    private final String topic;
    private final String groupId;
    private volatile boolean stopped = false;

    public OrderSource(String bootstrap, String topic, String groupId) {
        this.bootstrap = bootstrap;
        this.topic = topic;
        this.groupId = groupId;
    }

    public void stop() { stopped = true; }

    @Override public String component() { return "order_source"; }
    @Override public List<String> inports()  { return List.of("_trigger"); }
    @Override public List<String> outports() { return List.of("order"); }

    @Override public void run(ActorCallContext ctx) {
        Properties p = new Properties();
        p.put(ConsumerConfig.BOOTSTRAP_SERVERS_CONFIG, bootstrap);
        p.put(ConsumerConfig.GROUP_ID_CONFIG, groupId);
        p.put(ConsumerConfig.KEY_DESERIALIZER_CLASS_CONFIG, StringDeserializer.class.getName());
        p.put(ConsumerConfig.VALUE_DESERIALIZER_CLASS_CONFIG, StringDeserializer.class.getName());
        p.put(ConsumerConfig.AUTO_OFFSET_RESET_CONFIG, "earliest");

        try (KafkaConsumer<String, String> consumer = new KafkaConsumer<>(p)) {
            consumer.subscribe(List.of(topic));
            while (!stopped) {
                var records = consumer.poll(Duration.ofMillis(500));
                for (ConsumerRecord<String, String> r : records) {
                    ctx.send("order", Message.string(r.value()));
                }
            }
        }
        // Only reached on stop() — done() so the runtime can release.
        ctx.done();
    }
}
