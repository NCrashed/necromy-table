use crate::{
    AudioPlayer, Decodable, DefaultSpatialScale, GlobalVolume, PlaybackMode, PlaybackSettings,
    SpatialAudioSink, SpatialListener,
};
use bevy_asset::{Asset, Assets};
use bevy_ecs::{prelude::*, system::SystemParam};
use bevy_math::Vec3;
use bevy_transform::prelude::GlobalTransform;
use rodio::{DeviceSinkBuilder, MixerDeviceSink, Player, Source, SpatialPlayer};
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};
use std::time::{Duration, Instant};
use tracing::{info, warn};

use crate::{AudioSink, AudioSinkPlayback};

/// Used internally to play audio on the current "audio device"
#[derive(Resource)]
pub(crate) struct AudioOutput {
    stream: Option<MixerDeviceSink>,
    /// Set by the stream's error callback: the device went away.
    broken: Arc<AtomicBool>,
    /// What the output was opened on (`device_key`).
    device: Option<String>,
    /// When the device was last looked at.
    checked: Option<Instant>,
}

impl Default for AudioOutput {
    fn default() -> Self {
        let broken = Arc::new(AtomicBool::new(false));
        let flag = broken.clone();
        let on_error = move |err: rodio::cpal::StreamError| {
            if !matches!(err, rodio::cpal::StreamError::BufferUnderrun) {
                warn!("audio stream error: {err}");
                flag.store(true, Ordering::Relaxed);
            }
        };
        let stream = DeviceSinkBuilder::from_default_device()
            .and_then(|b| b.with_error_callback(on_error).open_sink_or_fallback())
            .or_else(|_| DeviceSinkBuilder::open_default_sink())
            .inspect_err(|_err| {
                warn!("No audio device found.");
            })
            .map(|mut s| {
                s.log_on_drop(false);
                s
            })
            .ok();
        Self {
            stream,
            broken,
            device: device_key(),
            checked: None,
        }
    }
}

/// Asks for the audio output to be opened again on the present default
/// device. Patched in by necromy-table (`patches/bevy_audio`), with the rest
/// of the reconnecting.
#[derive(Message, Clone, Copy, Debug, Default)]
pub struct ReopenAudioOutput;

/// What the default output is: its id, and on Linux the sound cards there
/// are. Through ALSA's `default` (PipeWire, PulseAudio) the id never changes,
/// and an old stream stays on the old sink when headphones come in.
fn device_key() -> Option<String> {
    use rodio::cpal::traits::{DeviceTrait, HostTrait};
    let id = rodio::cpal::default_host()
        .default_output_device()
        .and_then(|d| d.id().ok())
        .map(|id| format!("{id:?}"));
    #[cfg(target_os = "linux")]
    let id = Some(format!(
        "{id:?} {}",
        std::fs::read_to_string("/proc/asound/cards").unwrap_or_default()
    ));
    id
}

/// Every two seconds: when the default device changed, the stream failed or
/// someone asked (`ReopenAudioOutput`), the output is opened again and every
/// sound and piece of music starts anew on it.
pub(crate) fn reconnect_audio_output(
    mut output: ResMut<AudioOutput>,
    mut asked: MessageReader<ReopenAudioOutput>,
    sinks: Query<Entity, Or<(With<AudioSink>, With<SpatialAudioSink>)>>,
    mut commands: Commands,
) {
    let asked = asked.read().count() > 0;
    let now = Instant::now();
    if !asked && output.checked.is_some_and(|t| now - t < Duration::from_secs(2)) {
        return;
    }
    output.checked = Some(now);
    let device = device_key();
    let broken = output.broken.load(Ordering::Relaxed);
    if !asked && !broken && device == output.device {
        return;
    }
    info!(
        "audio output: reopening (asked: {asked}, failed: {broken}, device changed: {})",
        device != output.device
    );
    // The old stream goes first: some backends hold on to the device.
    output.stream = None;
    *output = AudioOutput::default();
    output.checked = Some(now);
    for entity in &sinks {
        commands
            .entity(entity)
            .remove::<(AudioSink, SpatialAudioSink)>();
    }
}

/// Marker for internal use, to despawn entities when playback finishes.
#[derive(Component, Default)]
pub struct PlaybackDespawnMarker;

/// Marker for internal use, to remove audio components when playback finishes.
#[derive(Component, Default)]
pub struct PlaybackRemoveMarker;

#[derive(SystemParam)]
pub(crate) struct EarPositions<'w, 's> {
    pub(crate) query: Query<'w, 's, (Entity, &'static GlobalTransform, &'static SpatialListener)>,
}

impl<'w, 's> EarPositions<'w, 's> {
    /// Gets a set of transformed ear positions.
    ///
    /// If there are no listeners, use the default values. If a user has added multiple
    /// listeners for whatever reason, we will return the first value.
    pub(crate) fn get(&self) -> (Vec3, Vec3) {
        let (left_ear, right_ear) = self
            .query
            .iter()
            .next()
            .map(|(_, transform, settings)| {
                (
                    transform.transform_point(settings.left_ear_offset),
                    transform.transform_point(settings.right_ear_offset),
                )
            })
            .unwrap_or_else(|| {
                let settings = SpatialListener::default();
                (settings.left_ear_offset, settings.right_ear_offset)
            });

        (left_ear, right_ear)
    }

    pub(crate) fn multiple_listeners(&self) -> bool {
        self.query.iter().len() > 1
    }
}

/// Plays "queued" audio through the [`AudioOutput`] resource.
///
/// "Queued" audio is any audio entity (with an [`AudioPlayer`] component) that does not have an
/// [`AudioSink`]/[`SpatialAudioSink`] component.
///
/// This system detects such entities, checks if their source asset
/// data is available, and creates/inserts the sink.
pub(crate) fn play_queued_audio_system<Source: Asset + Decodable>(
    audio_output: Res<AudioOutput>,
    audio_sources: Res<Assets<Source>>,
    global_volume: Res<GlobalVolume>,
    query_nonplaying: Query<
        (
            Entity,
            &AudioPlayer<Source>,
            &PlaybackSettings,
            Option<&GlobalTransform>,
        ),
        (Without<AudioSink>, Without<SpatialAudioSink>),
    >,
    ear_positions: EarPositions,
    default_spatial_scale: Res<DefaultSpatialScale>,
    mut commands: Commands,
) where
    f32: rodio::cpal::FromSample<rodio::Sample>,
{
    let Some(stream) = audio_output.stream.as_ref() else {
        // audio output unavailable; cannot play sound
        return;
    };
    let mixer = stream.mixer();

    for (entity, source_handle, settings, maybe_emitter_transform) in &query_nonplaying {
        let Some(audio_source) = audio_sources.get(&source_handle.0) else {
            continue;
        };
        // audio data is available (has loaded), begin playback and insert sink component
        if settings.spatial {
            let (left_ear, right_ear) = ear_positions.get();

            // We can only use one `SpatialListener`. If there are more than that, then
            // the user may have made a mistake.
            if ear_positions.multiple_listeners() {
                warn!(
                    "Multiple SpatialListeners found. Using {}.",
                    ear_positions.query.iter().next().unwrap().0
                );
            }

            let scale = settings.spatial_scale.unwrap_or(default_spatial_scale.0).0;

            let emitter_translation = if let Some(emitter_transform) = maybe_emitter_transform {
                (emitter_transform.translation() * scale).into()
            } else {
                warn!("Spatial AudioPlayer with no GlobalTransform component. Using zero.");
                Vec3::ZERO.into()
            };

            let sink = SpatialPlayer::connect_new(
                mixer,
                emitter_translation,
                (left_ear * scale).into(),
                (right_ear * scale).into(),
            );

            let decoder = audio_source.decoder();

            match settings.mode {
                PlaybackMode::Loop => match (settings.start_position, settings.duration) {
                    // custom start position and duration
                    (Some(start_position), Some(duration)) => sink.append(
                        decoder
                            .skip_duration(start_position)
                            .take_duration(duration)
                            .repeat_infinite(),
                    ),

                    // custom start position
                    (Some(start_position), None) => {
                        sink.append(decoder.skip_duration(start_position).repeat_infinite());
                    }

                    // custom duration
                    (None, Some(duration)) => {
                        sink.append(decoder.take_duration(duration).repeat_infinite());
                    }

                    // full clip
                    (None, None) => sink.append(decoder.repeat_infinite()),
                },
                PlaybackMode::Once | PlaybackMode::Despawn | PlaybackMode::Remove => {
                    match (settings.start_position, settings.duration) {
                        (Some(start_position), Some(duration)) => sink.append(
                            decoder
                                .skip_duration(start_position)
                                .take_duration(duration),
                        ),

                        (Some(start_position), None) => {
                            sink.append(decoder.skip_duration(start_position));
                        }

                        (None, Some(duration)) => sink.append(decoder.take_duration(duration)),

                        (None, None) => sink.append(decoder),
                    }
                }
            }

            let mut sink = SpatialAudioSink::new(sink);

            if settings.muted {
                sink.mute();
            }

            sink.set_speed(settings.speed);
            sink.set_volume(settings.volume * global_volume.volume);

            if settings.paused {
                sink.pause();
            }

            match settings.mode {
                PlaybackMode::Loop | PlaybackMode::Once => commands.entity(entity).insert(sink),
                PlaybackMode::Despawn => commands
                    .entity(entity)
                    // PERF: insert as bundle to reduce archetype moves
                    .insert((sink, PlaybackDespawnMarker)),
                PlaybackMode::Remove => commands
                    .entity(entity)
                    // PERF: insert as bundle to reduce archetype moves
                    .insert((sink, PlaybackRemoveMarker)),
            };
        } else {
            let sink = Player::connect_new(mixer);
            let decoder = audio_source.decoder();

            match settings.mode {
                PlaybackMode::Loop => match (settings.start_position, settings.duration) {
                    // custom start position and duration
                    (Some(start_position), Some(duration)) => sink.append(
                        decoder
                            .skip_duration(start_position)
                            .take_duration(duration)
                            .repeat_infinite(),
                    ),

                    // custom start position
                    (Some(start_position), None) => {
                        sink.append(decoder.skip_duration(start_position).repeat_infinite());
                    }

                    // custom duration
                    (None, Some(duration)) => {
                        sink.append(decoder.take_duration(duration).repeat_infinite());
                    }

                    // full clip
                    (None, None) => sink.append(decoder.repeat_infinite()),
                },
                PlaybackMode::Once | PlaybackMode::Despawn | PlaybackMode::Remove => {
                    match (settings.start_position, settings.duration) {
                        (Some(start_position), Some(duration)) => sink.append(
                            decoder
                                .skip_duration(start_position)
                                .take_duration(duration),
                        ),

                        (Some(start_position), None) => {
                            sink.append(decoder.skip_duration(start_position));
                        }

                        (None, Some(duration)) => sink.append(decoder.take_duration(duration)),

                        (None, None) => sink.append(decoder),
                    }
                }
            }

            let mut sink = AudioSink::new(sink);

            if settings.muted {
                sink.mute();
            }

            sink.set_speed(settings.speed);
            sink.set_volume(settings.volume * global_volume.volume);

            if settings.paused {
                sink.pause();
            }

            match settings.mode {
                PlaybackMode::Loop | PlaybackMode::Once => commands.entity(entity).insert(sink),
                PlaybackMode::Despawn => commands
                    .entity(entity)
                    // PERF: insert as bundle to reduce archetype moves
                    .insert((sink, PlaybackDespawnMarker)),
                PlaybackMode::Remove => commands
                    .entity(entity)
                    // PERF: insert as bundle to reduce archetype moves
                    .insert((sink, PlaybackRemoveMarker)),
            };
        }
    }
}

pub(crate) fn cleanup_finished_audio<T: Decodable + Asset>(
    mut commands: Commands,
    query_nonspatial_despawn: Query<
        (Entity, &AudioSink),
        (With<PlaybackDespawnMarker>, With<AudioPlayer<T>>),
    >,
    query_spatial_despawn: Query<
        (Entity, &SpatialAudioSink),
        (With<PlaybackDespawnMarker>, With<AudioPlayer<T>>),
    >,
    query_nonspatial_remove: Query<
        (Entity, &AudioSink),
        (With<PlaybackRemoveMarker>, With<AudioPlayer<T>>),
    >,
    query_spatial_remove: Query<
        (Entity, &SpatialAudioSink),
        (With<PlaybackRemoveMarker>, With<AudioPlayer<T>>),
    >,
) {
    for (entity, sink) in &query_nonspatial_despawn {
        if sink.sink.empty() {
            commands.entity(entity).despawn();
        }
    }
    for (entity, sink) in &query_spatial_despawn {
        if sink.sink.empty() {
            commands.entity(entity).despawn();
        }
    }
    for (entity, sink) in &query_nonspatial_remove {
        if sink.sink.empty() {
            commands.entity(entity).remove::<(
                AudioPlayer<T>,
                AudioSink,
                PlaybackSettings,
                PlaybackRemoveMarker,
            )>();
        }
    }
    for (entity, sink) in &query_spatial_remove {
        if sink.sink.empty() {
            commands.entity(entity).remove::<(
                AudioPlayer<T>,
                SpatialAudioSink,
                PlaybackSettings,
                PlaybackRemoveMarker,
            )>();
        }
    }
}

/// Run Condition to only play audio if the audio output is available
pub(crate) fn audio_output_available(audio_output: Res<AudioOutput>) -> bool {
    audio_output.stream.is_some()
}

/// Updates spatial audio sinks when emitter positions change.
pub(crate) fn update_emitter_positions(
    mut emitters: Query<
        (&GlobalTransform, &SpatialAudioSink, &PlaybackSettings),
        Or<(Changed<GlobalTransform>, Changed<PlaybackSettings>)>,
    >,
    default_spatial_scale: Res<DefaultSpatialScale>,
) {
    for (transform, sink, settings) in emitters.iter_mut() {
        let scale = settings.spatial_scale.unwrap_or(default_spatial_scale.0).0;

        let translation = transform.translation() * scale;
        sink.set_emitter_position(translation);
    }
}

/// Updates spatial audio sink ear positions when spatial listeners change.
pub(crate) fn update_listener_positions(
    mut emitters: Query<(&SpatialAudioSink, &PlaybackSettings)>,
    changed_listener: Query<
        (),
        (
            Or<(
                Changed<SpatialListener>,
                Changed<GlobalTransform>,
                Changed<PlaybackSettings>,
            )>,
            With<SpatialListener>,
        ),
    >,
    ear_positions: EarPositions,
    default_spatial_scale: Res<DefaultSpatialScale>,
) {
    if !default_spatial_scale.is_changed() && changed_listener.is_empty() {
        return;
    }

    let (left_ear, right_ear) = ear_positions.get();

    for (sink, settings) in emitters.iter_mut() {
        let scale = settings.spatial_scale.unwrap_or(default_spatial_scale.0).0;

        sink.set_ears_position(left_ear * scale, right_ear * scale);
    }
}
