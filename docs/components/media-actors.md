# Media Actors

Reflow provides native media actors for handling image, audio, and video input in workflows. These actors accept media data (raw bytes or URLs), extract metadata, and pass the enriched data downstream.

## Actors

### ImageInputActor (`tpl_image_input`)

Handles image input with metadata extraction.

**Template ID:** `tpl_image_input`

**Ports:**
- Input: `In` — image data (binary or URL)
- Output: `Out` — image with extracted metadata, `Error`

**Extracted metadata:**
- Dimensions (width, height)
- Format (JPEG, PNG, WebP, etc.)
- File size
- EXIF data (when available)

### AudioInputActor (`tpl_audio_input`)

Handles audio input with metadata extraction.

**Template ID:** `tpl_audio_input`

**Ports:**
- Input: `In` — audio data (binary or URL)
- Output: `Out` — audio with extracted metadata, `Error`

**Extracted metadata:**
- Duration
- Format (MP3, WAV, OGG, etc.)
- Sample rate
- Channels
- File size

### VideoInputActor (`tpl_video_input`)

Handles video input with metadata extraction.

**Template ID:** `tpl_video_input`

**Ports:**
- Input: `In` — video data (binary or URL)
- Output: `Out` — video with extracted metadata, `Error`

**Extracted metadata:**
- Duration
- Resolution (width, height)
- Format/codec
- Frame rate
- File size

## Usage in Workflows

Media actors are registered as Zeal templates and appear in the Zeal IDE palette under the "reflow" category. They can be connected to other actors in a workflow graph:

```
[Image URL] → [ImageInputActor] → [DataTransformActor] → [HttpRequestActor]
```

## Template Registration

Media actors are registered alongside other native actors during ZIP session startup. Each gets a template entry with:

```rust
NodeTemplate {
    id: "tpl_image_input",
    type_name: "tpl_image_input",
    title: "image input",
    category: "reflow",
    icon: "cpu",
    runtime: Some(RuntimeRequirements {
        executor: "reflow",
        // ...
    }),
}
```

## Next Steps

- [Standard Component Library](./standard-library.md) - All native actors
- [Media / ML Stack](./ml-stack.md) - Tensor, CV, inference, and taskpack actors
- [API Service Actors](./api-actors.md) - Generated API actors
