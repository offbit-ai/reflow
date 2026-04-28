// Tutorial 08: a tiny offline polyphonic synthesizer driven by Reflow.
//
//   Driver (Conductor) ──► Mixer.meta   (voice metadata updates)
//                       ──► Mixer.tick  (per-block clock pulses)
//
// The mixer's `meta` inport receives `{voice_id, freq, gain}` updates
// — three at startup, more if a voice changes parameters mid-piece.
// Each one upserts into a pool keyed by voice_id. The pool is the
// mixer's persistent state map: it persists across ticks, and only
// changes when a voice updates.
//
// The mixer's `tick` inport pulses once per audio block. On every
// pulse the mixer reads the entire `voices` pool, generates samples
// for each active voice using the captured phase, and appends to a
// WAV file.
//
// This is the canonical use of `ctx.pool`: stable-id state shared by
// the actor with multiple upstream sources, where the consumer reads
// the whole map per tick. Adding a fourth voice is one extra
// `voice_meta_update` message — the mixer's port surface doesn't
// change.

#include <reflow/reflow.hpp>

#include <atomic>
#include <chrono>
#include <cmath>
#include <condition_variable>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <fstream>
#include <mutex>
#include <string>
#include <thread>
#include <vector>

namespace ch = std::chrono;

constexpr int    kSampleRate = 44100;
constexpr int    kBlockSize  = 128;
constexpr int    kNumBlocks  = 344;        // ≈ 1 second of audio
constexpr int    kVoiceCount = 3;
constexpr double kVoiceFreqs[kVoiceCount] = {261.6256, 329.6276, 391.9954};  // C4 / E4 / G4

// ─── tiny JSON helpers ─────────────────────────────────────────────────────

static double json_double(std::string_view body, std::string_view key) {
    std::string needle = std::string("\"") + std::string(key) + "\":";
    auto pos = body.find(needle);
    if (pos == std::string_view::npos) return 0.0;
    return std::stod(std::string(body.substr(pos + needle.size())));
}

static int64_t json_int(std::string_view body, std::string_view key = "data") {
    std::string needle = std::string("\"") + std::string(key) + "\":";
    auto pos = body.find(needle);
    if (pos == std::string_view::npos) return 0;
    return std::stoll(std::string(body.substr(pos + needle.size())));
}

// ─── WAV writer ────────────────────────────────────────────────────────────

static void write_wav_pcm16(const std::string& path,
                            const std::vector<int16_t>& samples,
                            int sample_rate) {
    std::ofstream f(path, std::ios::binary);
    auto write_u32 = [&](uint32_t v) { f.write(reinterpret_cast<const char*>(&v), 4); };
    auto write_u16 = [&](uint16_t v) { f.write(reinterpret_cast<const char*>(&v), 2); };

    const uint32_t data_bytes = static_cast<uint32_t>(samples.size() * sizeof(int16_t));
    f.write("RIFF", 4);
    write_u32(36 + data_bytes);
    f.write("WAVE", 4);
    f.write("fmt ", 4);
    write_u32(16);                        // fmt chunk size
    write_u16(1);                         // PCM
    write_u16(1);                         // mono
    write_u32(sample_rate);
    write_u32(sample_rate * 2);           // byte rate
    write_u16(2);                         // block align
    write_u16(16);                        // bits per sample
    f.write("data", 4);
    write_u32(data_bytes);
    f.write(reinterpret_cast<const char*>(samples.data()), data_bytes);
}

// ─── shared sink for the mixer's WAV output ───────────────────────────────

struct WavSink {
    std::vector<int16_t>      samples;
    std::mutex                mu;
    std::condition_variable   cv;
    std::atomic<int>          blocks_seen{0};
};

int main() {
    std::printf("reflow runtime %s\n", reflow::version().c_str());

    reflow::Network net;

    // ── Driver ──────────────────────────────────────────────────────────
    //
    // One actor that fires once at startup. It first publishes voice
    // metadata (three messages, one per voice) so the mixer's pool is
    // populated, then emits kNumBlocks tick messages. Doing both in
    // the same `run` guarantees ordering — voices appear in the pool
    // before any tick reads it.
    auto driver = reflow::Actor::from_callback(
        "driver", /*inports=*/{"_trigger"}, /*outports=*/{"meta", "tick"},
        [](reflow::Context& ctx) {
            // Voice metadata (3 small messages — fits comfortably in
            // the cap-50 outport channel; ctx.send for mid-tick flush
            // since the driver also publishes tick stream below).
            for (int v = 0; v < kVoiceCount; ++v) {
                char buf[128];
                std::snprintf(buf, sizeof(buf),
                              "{\"voice_id\":%d,\"freq\":%.4f,\"gain\":0.25}",
                              v, kVoiceFreqs[v]);
                ctx.send("meta", reflow::Message::object_from_json(buf));
            }

            // Tick stream — every block index packed into one stream so
            // the kNumBlocks frames don't saturate the bounded outport
            // channel. Streams have their own (here: unbounded) channel,
            // independent of the per-actor outport queue. We push every
            // frame BEFORE calling into_message — the producer's sender
            // is dropped at into_message, but everything we queued
            // beforehand stays available for the receiver to drain.
            auto stream = reflow::StreamProducer::create(
                /*buffer_size=*/0, "driver", "tick");
            for (int b = 0; b < kNumBlocks; ++b) {
                std::uint8_t bytes[4] = {
                    static_cast<std::uint8_t>(b & 0xff),
                    static_cast<std::uint8_t>((b >> 8) & 0xff),
                    static_cast<std::uint8_t>((b >> 16) & 0xff),
                    static_cast<std::uint8_t>((b >> 24) & 0xff),
                };
                stream.send_bytes(bytes, 4);
            }
            ctx.emit("tick", std::move(stream).into_message());
        });

    // ── Mixer ───────────────────────────────────────────────────────────
    //
    // Two inports — `meta` (voice metadata updates) and `tick` (clock
    // pulses). Pool tracks per-voice metadata; phase state lives in the
    // captured C++ array (single-threaded callback, no mutex needed).
    WavSink sink;
    auto phases = std::make_shared<std::vector<double>>(64, 0.0);  // grows as voice ids appear
    auto render_block = [phases, &sink](reflow::Context& ctx, int block_idx) {
        std::string pool = ctx.pool_get_json("voices");
        std::vector<float> mixed(kBlockSize, 0.0f);

        // Walk every entry in the `voices` pool. The JSON shape is
        // `{"0": {voice_id:0, freq:..., gain:...}, "1": ...}`. We use
        // `freq` rather than searching for a phase value — phase
        // persists in the captured `phases` array.
        std::size_t cursor = 0;
        while ((cursor = pool.find("\"freq\":", cursor)) != std::string::npos) {
            auto entry_start = pool.rfind('{', cursor);
            auto entry_end = pool.find('}', cursor);
            if (entry_start == std::string::npos || entry_end == std::string::npos) break;
            auto entry = std::string_view(pool).substr(
                entry_start, entry_end - entry_start + 1);

            int voice_id = static_cast<int>(json_int(entry, "voice_id"));
            double freq  = json_double(entry, "freq");
            double gain  = json_double(entry, "gain");
            if (voice_id < 0 || voice_id >= static_cast<int>(phases->size())) {
                cursor = entry_end + 1;
                continue;
            }
            double dphase = 2.0 * M_PI * freq / kSampleRate;
            double phase = (*phases)[voice_id];
            for (int i = 0; i < kBlockSize; ++i) {
                mixed[i] += static_cast<float>(std::sin(phase) * gain);
                phase += dphase;
            }
            (*phases)[voice_id] = phase;
            cursor = entry_end + 1;
        }

        // Soft fade in/out over the first/last 50 blocks so the edges
        // don't click.
        double env_in  = std::min(1.0, block_idx / 50.0);
        double env_out = std::min(1.0, (kNumBlocks - block_idx) / 50.0);
        double env = env_in * env_out;

        std::lock_guard<std::mutex> lk(sink.mu);
        sink.samples.reserve(sink.samples.size() + kBlockSize);
        for (float s : mixed) {
            float clamped = std::max(-1.0f, std::min(1.0f, s * static_cast<float>(env)));
            sink.samples.push_back(static_cast<int16_t>(clamped * 32767.0f));
        }
        sink.blocks_seen.fetch_add(1, std::memory_order_release);
        sink.cv.notify_one();

        ctx.emit("block", reflow::Message::integer(
            static_cast<int64_t>(sink.blocks_seen.load())));
    };

    auto mixer = reflow::Actor::from_callback(
        "mixer", /*inports=*/{"meta", "tick"}, /*outports=*/{"block"},
        [render_block](reflow::Context& ctx) {
            // ── meta: stash voice config in the pool ──────────────────
            if (auto m = ctx.take_input("meta")) {
                auto inner_opt = m->data_json();
                if (inner_opt) {
                    int voice_id = static_cast<int>(json_int(*inner_opt, "voice_id"));
                    ctx.pool_upsert("voices", std::to_string(voice_id), *inner_opt);
                }
            }
            // ── tick: a StreamHandle that delivers every block index ─
            //
            // The driver pushes one Data frame per block and ends the
            // stream. We pull frames in a loop right here — per-frame
            // rendering happens inside one `run()` invocation, but the
            // stream's own channel handles backpressure independently
            // of the actor's bounded outport.
            if (auto t = ctx.take_input("tick")) {
                auto reader_opt = reflow::StreamReader::from_message(*t);
                if (!reader_opt) return;   // not a stream — ignore
                auto& reader = *reader_opt;
                while (true) {
                    auto frame = reader.recv(/*timeout_ms=*/5000);
                    if (frame.kind == rfl_stream_frame_kind_End ||
                        frame.kind == rfl_stream_frame_kind_Closed ||
                        frame.kind == rfl_stream_frame_kind_Timeout) {
                        break;
                    }
                    if (frame.kind != rfl_stream_frame_kind_Data) continue;
                    if (frame.data.size() < 4) continue;
                    int block_idx =
                        static_cast<int>(frame.data[0]) |
                        (static_cast<int>(frame.data[1]) << 8) |
                        (static_cast<int>(frame.data[2]) << 16) |
                        (static_cast<int>(frame.data[3]) << 24);
                    render_block(ctx, block_idx);
                }
            }
        });

    // ── Wire it up ──────────────────────────────────────────────────────
    net.register_actor("tpl_driver", std::move(driver));
    net.register_actor("tpl_mixer",  std::move(mixer));

    net.add_node("driver", "tpl_driver");
    net.add_node("mixer",  "tpl_mixer");
    net.add_connection("driver", "meta", "mixer", "meta");
    net.add_connection("driver", "tick", "mixer", "tick");

    net.add_initial("driver", "_trigger", R"({"type":"Flow"})");
    net.start();

    // ── Wait for all blocks to land ─────────────────────────────────────
    {
        std::unique_lock<std::mutex> lk(sink.mu);
        sink.cv.wait_for(lk, ch::seconds(20), [&] {
            return sink.blocks_seen.load() >= kNumBlocks;
        });
    }
    net.shutdown();

    int got = sink.blocks_seen.load();
    std::printf("rendered %d blocks (%d expected)\n", got, kNumBlocks);
    if (got < kNumBlocks) {
        std::fprintf(stderr, "incomplete render — only %d of %d blocks landed\n",
                     got, kNumBlocks);
        return 1;
    }

    write_wav_pcm16("tutorial-08.wav", sink.samples, kSampleRate);
    std::printf("wrote tutorial-08.wav (%zu samples, %.2f s)\n",
                sink.samples.size(),
                sink.samples.size() / static_cast<double>(kSampleRate));
    return 0;
}
