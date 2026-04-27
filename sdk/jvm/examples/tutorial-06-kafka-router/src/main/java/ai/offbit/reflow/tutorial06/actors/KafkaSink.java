// Writes each input packet to its configured Kafka topic. One
// instance per output topic — the producer is constructed once at
// network start and lives for the network's lifetime.

package ai.offbit.reflow.tutorial06.actors;

import ai.offbit.reflow.Actor;
import ai.offbit.reflow.ActorCallContext;
import org.apache.kafka.clients.producer.KafkaProducer;
import org.apache.kafka.clients.producer.ProducerConfig;
import org.apache.kafka.clients.producer.ProducerRecord;
import org.apache.kafka.common.serialization.StringSerializer;

import java.util.List;
import java.util.Properties;

public class KafkaSink extends Actor {
    private final String bootstrap;
    private final String topic;
    private volatile KafkaProducer<String, String> producer;

    public KafkaSink(String bootstrap, String topic) {
        this.bootstrap = bootstrap;
        this.topic = topic;
    }

    public void close() {
        var p = producer;
        if (p != null) p.close();
    }

    @Override public String component() { return "kafka_sink_" + topic; }
    @Override public List<String> inports()  { return List.of("in"); }
    @Override public List<String> outports() { return List.of(); }

    @Override public void run(ActorCallContext ctx) {
        if (producer == null) {
            synchronized (this) {
                if (producer == null) {
                    Properties p = new Properties();
                    p.put(ProducerConfig.BOOTSTRAP_SERVERS_CONFIG, bootstrap);
                    p.put(ProducerConfig.KEY_SERIALIZER_CLASS_CONFIG, StringSerializer.class.getName());
                    p.put(ProducerConfig.VALUE_SERIALIZER_CLASS_CONFIG, StringSerializer.class.getName());
                    p.put(ProducerConfig.LINGER_MS_CONFIG, 5);
                    producer = new KafkaProducer<>(p);
                }
            }
        }
        String body = ctx.inputDataJson("in");
        // body is a JSON-quoted string (`"raw json"`); strip quoting.
        String value = body != null && body.length() >= 2 && body.charAt(0) == '"'
            ? body.substring(1, body.length() - 1).replace("\\\"", "\"")
            : body;
        producer.send(new ProducerRecord<>(topic, value));
        ctx.done();
    }
}
