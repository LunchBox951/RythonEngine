# Audio

The audio system uses kira for spatial 3D audio, replacing PythonEngine's PyOpenAL. Kira manages its own audio thread internally, so the engine communicates with it through a command-based interface. Audio commands are submitted from any thread and processed asynchronously by kira.


## Architecture

The AudioManager is a Module that initializes the kira audio engine on load. It exposes a command API for playing sounds, controlling volume, and positioning the 3D listener.

Unlike most engine systems, the AudioManager does not need a per-frame recurring task. Sound playback is fire-and-forget once started. The only per-frame work is updating the 3D listener position (to match the camera) and processing any queued commands.


## Audio Categories

Sounds are organized into four categories, each with independent volume control:

- **SFX**: Short sound effects (footsteps, impacts, UI clicks)
- **Dialogue**: Voice lines and speech
- **Music**: Background music tracks
- **Ambient**: Environmental sounds (wind, rain, crowd noise)

```python
import rython

# Set per-category volume
rython.audio.set_volume("sfx", 0.8)
rython.audio.set_volume("music", 0.5)
rython.audio.set_volume("ambient", 0.6)

# Master volume scales all categories
rython.audio.set_master_volume(1.0)
```


## Playing Sounds

```python
# Play a 2D sound (no spatial positioning)
rython.audio.play("ui/click.wav", category="sfx")

# Play a 3D spatial sound at a world position
rython.audio.play("weapons/explosion.wav",
    category="sfx",
    position=(10.0, 0.0, 5.0),
)

# Play looping music
handle = rython.audio.play("music/battle_theme.ogg",
    category="music",
    looping=True,
)

# Stop a specific sound
rython.audio.stop(handle)

# Stop all sounds in a category
rython.audio.stop_category("music")
```


## 3D Spatial Audio

For sounds played with a world position, kira applies distance attenuation and stereo panning based on the listener's position and orientation. The listener typically follows the camera.

```python
# Update listener position (usually done by the camera system each frame)
rython.audio.set_listener(
    position=(0, 10, -20),
    forward=(0, 0, 1),
    up=(0, 1, 0),
)
```

The maximum audible radius is configurable. Sounds beyond this distance are not played, saving audio source slots.


## Ambient Sound Groups

For ambient audio, the engine supports sound groups with multiple emitter positions. Only the closest emitters to the listener are actually played, using a culling algorithm:

```python
# Register an ambient group with multiple emitters
rython.audio.register_ambient_group(
    name="forest_birds",
    sound="ambient/birds.ogg",
    positions=[(10, 0, 5), (20, 0, -10), (35, 0, 15)],
    max_audible=3,
)
```


## Output Modes

The audio system supports multiple output modes:
- **Stereo**: Standard 2-channel output (default)
- **Surround 5.1**: 6-channel surround sound
- **Surround 5.1.2**: 8-channel with height channels

The output mode is set in configuration and cannot be changed at runtime.


## Supported Formats

The ResourceManager decodes audio files in the background using symphonia (via kira):
- WAV (uncompressed PCM)
- OGG Vorbis
- FLAC
- MP3


## Configuration

```json
{
    "audio": {
        "output_mode": "stereo",
        "master_volume": 1.0,
        "sfx_volume": 1.0,
        "dialogue_volume": 1.0,
        "music_volume": 1.0,
        "ambient_volume": 1.0,
        "max_sources": 32,
        "max_audible_radius": 100.0
    }
}
```

- `output_mode`: "stereo", "5.1", or "5.1.2"
- `master_volume`: Global volume multiplier (0.0 - 1.0)
- `*_volume`: Per-category volume multiplier
- `max_sources`: Maximum concurrent sounds
- `max_audible_radius`: Sounds beyond this distance (in world units) are culled


## Acceptance Tests

### T-AUD-01: Audio System Initialization
Initialize the AudioManager module with default config.
- Expected: kira audio manager is created without error
- Expected: The module enters LOADED state
- Expected: No audio device errors on a system with a sound output device

### T-AUD-02: Play Sound Returns Handle
Play a short WAV file. Capture the returned playback handle.
- Expected: The handle is not null/zero
- Expected: The handle uniquely identifies this playback instance

### T-AUD-03: Stop Sound by Handle
Play a looping sound. Capture the handle. Stop it using the handle.
- Expected: The sound stops playing
- Expected: Calling stop on the same handle again does not error (idempotent)

### T-AUD-04: Stop Category
Play 3 sounds in the "sfx" category and 2 in "music". Call `stop_category("sfx")`.
- Expected: All 3 SFX sounds stop
- Expected: The 2 music sounds continue playing

### T-AUD-05: Master Volume Scaling
Set master_volume to 0.5. Set sfx_volume to 1.0. Play an SFX sound.
- Expected: Effective volume for the sound is 0.5 * 1.0 = 0.5
- Expected: The kira sound instance's volume parameter equals 0.5

### T-AUD-06: Category Volume Independence
Set sfx_volume=0.8, music_volume=0.3, master_volume=1.0. Play one SFX and one music sound.
- Expected: SFX effective volume = 0.8
- Expected: Music effective volume = 0.3

### T-AUD-07: Master Volume Zero Mutes All
Set master_volume to 0.0. Play sounds in all 4 categories.
- Expected: All sounds have effective volume 0.0

### T-AUD-08: Max Sources Enforcement
Set max_sources=4. Attempt to play 6 sounds simultaneously.
- Expected: Only 4 sounds are playing at any time
- Expected: The 5th and 6th sounds are either queued, rejected, or replace the oldest/quietest

### T-AUD-09: Distance Culling
Set max_audible_radius=50.0. Set listener at origin. Play a spatial sound at position (100, 0, 0).
- Expected: The sound is NOT played (distance 100 > max radius 50)
- Expected: No error is raised (silent rejection)

### T-AUD-10: Distance Culling — Within Range
Set max_audible_radius=50.0. Listener at origin. Play spatial sound at (30, 0, 0).
- Expected: The sound IS played (distance 30 < max radius 50)

### T-AUD-11: Listener Position Update
Set listener at (0, 0, 0). Play a spatial sound at (10, 0, 0). Move listener to (10, 0, 0). Verify panning changes.
- Expected: When listener is at origin, the sound pans toward the right channel
- Expected: When listener moves to (10, 0, 0), the sound is centered (same position)

### T-AUD-12: Looping Sound Loops
Play a 0.5-second sound with looping=true. Wait 2 seconds.
- Expected: The sound is still playing after 2 seconds (it looped at least 3 times)

### T-AUD-13: Non-Looping Sound Stops
Play a 0.5-second sound with looping=false. Wait 2 seconds.
- Expected: The sound is no longer playing after 2 seconds

### T-AUD-14: Ambient Group Culling
Register an ambient group with 5 emitter positions and max_audible=2. Set listener near 2 of them.
- Expected: Exactly 2 emitters are playing (the 2 closest to the listener)
- Expected: Moving the listener closer to a different emitter swaps which ones are active

### T-AUD-15: Unsupported Format Handling
Attempt to play a file with an unsupported extension (e.g., ".aac").
- Expected: An error is returned (not a panic)
- Expected: The engine continues running
