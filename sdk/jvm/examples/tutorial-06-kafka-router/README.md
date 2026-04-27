# Tutorial 06 — Kafka stream router (Java)

Runnable code for [Real-world Reflow 06: Long-running stream router
on the JVM](../../../../docs/tutorials/real-world/06-kafka-router.md).

The shape — long-running graph, no per-request scaffolding — is the
opposite of [tutorial 05](../tutorial-05-spring-enrich/). One Reflow
network boots at startup and runs forever; the source actor's
Kafka poll loop drives ticks through the graph.

```
OrderSource ──► Router ──┬─► confirmed → orders.confirmed
                         ├─► cancelled → orders.cancelled
                         ├─► refunded  → orders.refunded
                         └─► other     → orders.dlq
```

## Run

Bring up Kafka and create the topics:

```sh
cd sdk/jvm/examples/tutorial-06-kafka-router
docker compose up -d
for t in orders orders.confirmed orders.cancelled orders.refunded orders.dlq; do
  docker exec tut06-kafka /opt/kafka/bin/kafka-topics.sh \
    --bootstrap-server localhost:9092 --create --topic $t \
    --partitions 1 --replication-factor 1
done
```

Start the router:

```sh
gradle run
```

Produce a few orders:

```sh
docker exec -i tut06-kafka /opt/kafka/bin/kafka-console-producer.sh \
  --bootstrap-server localhost:9092 --topic orders <<EOF
{"id":"a","status":"confirmed"}
{"id":"b","status":"cancelled"}
{"id":"c","status":"refunded"}
{"id":"d","status":"weird"}
EOF
```

Read each output topic:

```sh
for t in confirmed cancelled refunded dlq; do
  echo "=== orders.$t ==="
  docker exec tut06-kafka /opt/kafka/bin/kafka-console-consumer.sh \
    --bootstrap-server localhost:9092 --topic orders.$t \
    --from-beginning --max-messages 1 --timeout-ms 5000
done
```

Each input lands in exactly one output topic based on its `status` —
the routing policy is one method (`Router.run`).

Tear down:

```sh
docker compose down
```

## Why `ctx.send` instead of `ctx.emit`

`OrderSource.run()` polls Kafka in an infinite loop and never
returns. `ctx.emit` accumulates packets in a HashMap that drains
only when `ctx.done()` fires; for a never-returning source, emits
would be silently lost. `ctx.send` pushes straight to the outport
channel and is the right tool for continuously-publishing source
actors.

This requires JVM SDK ≥ 0.2.6.
