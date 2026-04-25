# Publishing the JVM SDK to Maven Central

Coordinates we'll be publishing under:

- Group ID:    `ai.offbit`
- Artifact ID: `reflow`
- Version:     matches the `sdk/jvm/v*` git tag (e.g. `0.2.0`)

End users add it like:

```kotlin
dependencies {
    implementation("ai.offbit:reflow:0.2.0")
}
```

## Why this is more involved than npm/PyPI

Maven Central requires every release to:

1. Be **signed** with a GPG key whose public half is on a keyserver.
2. Carry a **POM** with mandatory metadata (name, description, url,
   license, scm, developers).
3. Ship `-sources.jar` and `-javadoc.jar` alongside the main jar.
4. Be uploaded to the **Sonatype Central Portal**, where a closing /
   release dance moves it from staging to the public index.

Plus we have a fifth wrinkle: the SDK ships **native libraries**
(`libreflow_rt_jvm.{dylib,so,dll}`) that JNI dlopens at runtime. Maven
Central artifacts can embed them as classpath resources or attach
per-platform classifier jars.

## One-time account + namespace setup

### 1. Sonatype Central Portal account

1. Sign up at <https://central.sonatype.com> (use the **same** email
   that will later be your GPG key's UID).
2. **Settings → User Token** → generate a token. You'll get a
   `username` + `password` pair (these are *not* your portal login
   credentials — they're machine credentials).
3. Save them in the GitHub repo as secrets:
   - `SONATYPE_USERNAME`
   - `SONATYPE_PASSWORD`

### 2. Claim the `ai.offbit` namespace

1. **Settings → Namespaces → Add Namespace**.
2. Choose either:
   - DNS verification — add a TXT record on `offbit.ai` proving you
     own the domain (recommended; one-time).
   - GitHub verification — claim `io.github.offbit-ai` (auto-verified
     because you own the org). This means the group ID becomes
     `io.github.offbit-ai`, not `ai.offbit`. Trade-off: zero DNS
     setup but a less idiomatic group id.
3. Wait for verification (DNS: minutes; GitHub: instant).

If you go with GitHub verification, change `group = "ai.offbit"` to
`group = "io.github.offbit-ai"` in `sdk/jvm/build.gradle.kts` and
update this guide accordingly.

### 3. GPG signing key

```sh
# Generate a publishing key (RSA 4096, no expiry for CI simplicity).
gpg --full-generate-key
#   Real name:    Offbit AI
#   Email:        damilare@offbit.ai      <- must match Sonatype account
#   Passphrase:   <strong; you'll save it as a CI secret>

# Confirm the key id.
gpg --list-secret-keys --keyid-format=long
# sec   rsa4096/AB12CD34EF567890 2026-04-25 [SC]
#       └─ this is your KEY_ID

# Push the public half to keyservers Maven Central trusts.
gpg --keyserver keys.openpgp.org --send-keys AB12CD34EF567890
gpg --keyserver keyserver.ubuntu.com --send-keys AB12CD34EF567890

# Export the secret key in ASCII for CI.
# Export armored, then base64-encode to a single line. We use base64
# because GitHub-secret multi-line values occasionally come back with
# CR-LF / trimmed whitespace, which makes the vanniktech plugin
# fail with "Could not read PGP secret key".
gpg --armor --export-secret-keys AB12CD34EF567890 | base64 | tr -d '\n' > /tmp/signing.key.b64
```

Add three secrets to the GitHub repo:

| Secret name        | Value |
|--------------------|-------|
| `SIGNING_KEY_ID`   | The 16-char id (last 16 hex chars of the long key id) |
| `SIGNING_KEY`      | The single-line base64 content of `/tmp/signing.key.b64` (no newlines) |
| `SIGNING_PASSWORD` | The passphrase you typed during `gpg --full-generate-key` |

Delete `/tmp/signing.key.b64` from disk afterwards. The CI workflow
decodes the secret back to the multi-line PGP block before invoking
gradle.

## Gradle publish plugin

The cleanest way to wire the metadata + signing is
[`com.vanniktech.maven.publish`][vmp] — it fills in the POM
automatically from the `pom { ... }` block, generates `-sources.jar`
and `-javadoc.jar`, signs everything, and uploads to the Central
Portal in one task.

[vmp]: https://github.com/vanniktech/gradle-maven-publish-plugin

### `sdk/jvm/build.gradle.kts`

Already wired — `id("com.vanniktech.maven.publish") version "0.30.0"`
plus a `mavenPublishing { ... }` block at the bottom of the file
configures coordinates, POM metadata, signing, and the
staging-then-release dance against the Sonatype Central Portal.

`automaticRelease = false` means the first release stages on the
Portal so you can inspect/promote manually. Flip to `true` once
you're confident the artifact looks right and want pushes to go
straight to Central.

The plugin reads four properties at task time:

- `mavenCentralUsername` — Sonatype user token username
- `mavenCentralPassword` — Sonatype user token password
- `signingInMemoryKey`, `signingInMemoryKeyId`, `signingInMemoryKeyPassword`

We pass them in CI as Gradle properties via env vars; see the workflow
below.

## Bundling the native library

Each release jar must contain the `libreflow_rt_jvm.{dylib,so,dll}`
binaries the JNI layer dlopens at runtime. Two patterns to choose
from:

**A. Fat jar (one artifact, all platforms).** Place each platform's
   library at `src/main/resources/native/<os>-<arch>/<libname>` before
   `jar`. JNI loader at startup detects the host triple, extracts the
   matching binary to `java.io.tmpdir`, then `System.load`s it.
   Single `reflow-0.2.0.jar` consumers depend on; jar ~30 MB.

**B. Per-platform classifiers.** Publish
   `reflow-0.2.0-darwin-arm64.jar` etc. alongside the main jar
   (which holds only Java/Kotlin classes). Gradle dependency uses
   `classifier`. Smaller per-platform downloads but consumer must pick
   the right classifier (or use a Gradle plugin like `osdetector`).

Pick one and commit. **Recommendation: A** — Maven Central artifacts
historically use fat jars for native deps (e.g. `tensorflow-core`,
`onnxruntime`), and JVM users tend to find classifiers confusing. The
~30 MB extra is fine for SDK-style usage.

The CI workflow below assumes pattern A.

## CI workflow

Already wired — see [`.github/workflows/publish-jvm.yml`](../../.github/workflows/publish-jvm.yml).
Triggers on `sdk/jvm/v*` tag push (or `workflow_dispatch` for build
verification without uploading), builds the native lib for all 5
triples in parallel, stages them under
`src/main/resources/native/<res>/`, and runs
`./gradlew publishAndReleaseToMavenCentral` with the secrets above
fed through `ORG_GRADLE_PROJECT_*` env vars.

## JNI loader

The fat-jar approach needs a tiny loader on the Java side. Add to
`sdk/jvm/src/main/java/ai/offbit/reflow/Reflow.java` (or wherever the
SDK's lifecycle init lives):

```java
private static volatile boolean loaded = false;

static synchronized void ensureLoaded() {
    if (loaded) return;
    String os   = System.getProperty("os.name").toLowerCase();
    String arch = System.getProperty("os.arch").toLowerCase();
    String dir, lib;
    if (os.contains("mac"))            dir = arch.contains("aarch64") || arch.contains("arm64") ? "darwin-arm64" : "darwin-x86_64";
    else if (os.contains("nux"))       dir = arch.contains("aarch64") ? "linux-aarch64" : "linux-x86_64";
    else if (os.contains("win"))       dir = "windows-x86_64";
    else throw new UnsatisfiedLinkError("unsupported platform: " + os + " " + arch);
    if (dir.startsWith("darwin"))      lib = "libreflow_rt_jvm.dylib";
    else if (dir.startsWith("linux"))  lib = "libreflow_rt_jvm.so";
    else                               lib = "reflow_rt_jvm.dll";

    String resource = "/native/" + dir + "/" + lib;
    try (var in = Reflow.class.getResourceAsStream(resource)) {
        if (in == null) throw new UnsatisfiedLinkError("missing classpath resource: " + resource);
        var tmp = java.io.File.createTempFile("libreflow_rt_jvm", lib.substring(lib.lastIndexOf('.')));
        tmp.deleteOnExit();
        try (var out = new java.io.FileOutputStream(tmp)) { in.transferTo(out); }
        System.load(tmp.getAbsolutePath());
    } catch (java.io.IOException e) {
        throw new UnsatisfiedLinkError("extracting " + resource + ": " + e.getMessage());
    }
    loaded = true;
}
```

Every public entry point (`Network`, `Templates`, `Packs`, …) calls
`Reflow.ensureLoaded()` in its static initializer.

## Tagging a release

```sh
# Bump version in sdk/jvm/build.gradle.kts (and reflect in sdk/jvm/README.md).
git commit -am "JVM SDK 0.2.0"

git tag sdk/jvm/v0.2.0
git push origin sdk/jvm/v0.2.0
```

The `publish-jvm` workflow:

1. Builds native libs for all 5 triples in parallel.
2. Stages them into `src/main/resources/native/<res>/` on the publish
   runner.
3. `./gradlew publishToMavenCentral` builds the jar (now containing
   all 5 native libs as resources), signs it, signs the
   `-sources.jar`/`-javadoc.jar`, and uploads to the Sonatype Central
   Portal.
4. With `automaticRelease = true`, the staging repo is closed and
   released to Maven Central in one step. (Set to `false` for the
   first release if you want to verify the staged artifacts manually
   before they go public — log into the Portal, inspect, click
   "Release".)
5. Maven Central indexing typically takes 10–30 minutes; the
   artifact is fetchable from
   `https://repo1.maven.org/maven2/ai/offbit/reflow/0.2.0/`
   after that.

## Fallback / debug commands

```sh
# Dry run locally — produces signed artifacts in ~/.m2 without uploading.
cd sdk/jvm
./gradlew publishToMavenLocal

# Verify the produced jar has all 5 native libs:
unzip -l ~/.m2/repository/ai/offbit/reflow/0.2.0/reflow-0.2.0.jar | grep native/
```

## Verifying the GPG keychain in CI

If the publish fails with `gpg: signing failed: No secret key`, the
in-memory key isn't being decoded. Common causes:

- The `SIGNING_KEY` secret isn't valid base64 (e.g. line-wrapped to
  multiple lines, or pasted with a trailing newline). Re-encode with
  `gpg --armor --export-secret-keys KEY_ID | base64 | tr -d '\n'`
  and paste the resulting single line into the GitHub secret.
- The `SIGNING_KEY_ID` is wrong length. Use the **last 16 hex
  characters** of the long-form id, not the short 8-char id.
- The passphrase has a special character that GitHub's secret store
  decoded oddly — regenerate without `$`, `&`, or unicode.
