#[cfg(not(unix))]
use std::io::Write;
use std::process::{Child, Command, ExitStatus, Stdio};
#[cfg(target_os = "linux")]
use std::time::{Duration, Instant};

use anyhow::{Context, Result, bail};

use crate::subsonic::StreamTarget;

#[cfg(target_os = "linux")]
const LIVE_VOLUME_LOOKUP_THROTTLE: Duration = Duration::from_millis(80);
#[cfg(target_os = "linux")]
const LIVE_VOLUME_STARTUP_WAIT: Duration = Duration::from_millis(320);
#[cfg(target_os = "linux")]
const LIVE_VOLUME_STARTUP_POLL: Duration = Duration::from_millis(20);

pub struct PlaybackEngine {
    child: Option<Child>,
    paused: bool,
    use_fast_start: bool,
    reported_volume_update: Option<u8>,
    #[cfg(target_os = "linux")]
    live_volume_backend_available: bool,
    #[cfg(target_os = "linux")]
    pending_live_volume: Option<u8>,
    #[cfg(target_os = "linux")]
    sink_input_id: Option<u32>,
    #[cfg(target_os = "linux")]
    last_sink_lookup: Option<Instant>,
}

impl PlaybackEngine {
    pub fn new() -> Self {
        let use_fast_start = env_flag_enabled("NAVTUI_FAST_START")
            .or_else(|| env_flag_enabled("SUBSONIC_TUI_FAST_START"))
            .unwrap_or(false);

        Self {
            child: None,
            paused: false,
            use_fast_start,
            reported_volume_update: None,
            #[cfg(target_os = "linux")]
            live_volume_backend_available: detect_live_volume_backend(),
            #[cfg(target_os = "linux")]
            pending_live_volume: None,
            #[cfg(target_os = "linux")]
            sink_input_id: None,
            #[cfg(target_os = "linux")]
            last_sink_lookup: None,
        }
    }

    pub fn play_target(
        &mut self,
        target: &StreamTarget,
        volume_percent: u8,
        seek_seconds: f64,
    ) -> Result<()> {
        self.start_target(
            target,
            self.use_fast_start,
            volume_percent,
            seek_seconds,
            true,
        )
    }

    pub fn play_target_compat(
        &mut self,
        target: &StreamTarget,
        volume_percent: u8,
        seek_seconds: f64,
    ) -> Result<()> {
        self.use_fast_start = false;
        self.start_target(target, false, volume_percent, seek_seconds, true)
    }

    pub fn play_target_seek(
        &mut self,
        target: &StreamTarget,
        volume_percent: u8,
        seek_seconds: f64,
    ) -> Result<()> {
        self.start_target(
            target,
            self.use_fast_start,
            volume_percent,
            seek_seconds,
            true,
        )
    }

    pub fn play_target_compat_seek(
        &mut self,
        target: &StreamTarget,
        volume_percent: u8,
        seek_seconds: f64,
    ) -> Result<()> {
        self.use_fast_start = false;
        self.start_target(target, false, volume_percent, seek_seconds, true)
    }

    pub fn toggle_pause(&mut self) -> Result<bool> {
        if self.child.is_none() {
            bail!("no active playback process");
        }

        #[cfg(unix)]
        {
            let child = self
                .child
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("no active playback process"))?;
            let signal = if self.paused {
                libc::SIGCONT
            } else {
                libc::SIGSTOP
            };

            // SAFETY: the PID comes from std::process::Child and the signal constants are valid.
            let rc = unsafe { libc::kill(child.id() as i32, signal) };
            if rc != 0 {
                bail!("failed to signal ffplay process");
            }

            self.paused = !self.paused;
            Ok(self.paused)
        }

        #[cfg(not(unix))]
        {
            let child = self
                .child
                .as_mut()
                .ok_or_else(|| anyhow::anyhow!("no active playback process"))?;
            send_ffplay_toggle_pause(child)?;
            self.paused = !self.paused;
            Ok(self.paused)
        }
    }

    pub fn paused(&self) -> bool {
        self.paused
    }

    pub fn has_active_playback(&self) -> bool {
        self.child.is_some()
    }

    pub fn take_reported_volume_update(&mut self) -> Option<u8> {
        self.reported_volume_update.take()
    }

    pub fn set_live_volume(&mut self, volume_percent: u8) -> Result<bool> {
        if self.child.is_none() {
            return Ok(false);
        }

        #[cfg(target_os = "linux")]
        {
            if !self.live_volume_backend_available {
                return Ok(false);
            }
            self.pending_live_volume = Some(volume_percent.clamp(0, 100));
            self.try_apply_pending_live_volume()
        }

        #[cfg(not(target_os = "linux"))]
        {
            let _ = volume_percent;
            Ok(false)
        }
    }

    fn launch_ffplay_volume(&self, requested_volume: u8) -> u8 {
        #[cfg(target_os = "linux")]
        {
            // When live backend is available, keep ffplay at unity gain and control
            // perceived volume exclusively through sink/sink-input volume.
            if self.live_volume_backend_available {
                return 100;
            }
        }
        requested_volume.clamp(0, 100)
    }

    fn start_target(
        &mut self,
        target: &StreamTarget,
        fast_start: bool,
        volume_percent: u8,
        seek_seconds: f64,
        _wait_for_live_volume: bool,
    ) -> Result<()> {
        self.stop()?;
        let launch_volume = self.launch_ffplay_volume(volume_percent);
        let child = spawn_ffplay(target, fast_start, launch_volume, seek_seconds)
            .context("failed to start ffplay")?;
        self.child = Some(child);
        self.paused = false;
        self.reported_volume_update = Some(launch_volume);
        #[cfg(target_os = "linux")]
        {
            self.pending_live_volume = None;
            self.sink_input_id = None;
            self.last_sink_lookup = None;
            if self.live_volume_backend_available {
                let _ = self.set_live_volume(volume_percent)?;
                if _wait_for_live_volume {
                    self.wait_for_pending_live_volume()?;
                }
            }
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn wait_for_pending_live_volume(&mut self) -> Result<()> {
        if self.pending_live_volume.is_none() {
            return Ok(());
        }

        let deadline = Instant::now() + LIVE_VOLUME_STARTUP_WAIT;
        while self.pending_live_volume.is_some() && self.child.is_some() {
            if self.try_apply_pending_live_volume()? {
                break;
            }
            if Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(LIVE_VOLUME_STARTUP_POLL);
        }
        Ok(())
    }

    #[cfg(target_os = "linux")]
    fn try_apply_pending_live_volume(&mut self) -> Result<bool> {
        let Some(target_volume) = self.pending_live_volume else {
            return Ok(false);
        };
        let Some(child) = self.child.as_ref() else {
            self.pending_live_volume = None;
            self.sink_input_id = None;
            self.last_sink_lookup = None;
            return Ok(false);
        };

        let volume_arg = format!("{}%", target_volume.clamp(0, 100));

        if let Some(sink_input_id) = self.sink_input_id {
            if set_sink_input_volume(sink_input_id, &volume_arg)? {
                self.pending_live_volume = None;
                self.reported_volume_update = Some(target_volume);
                return Ok(true);
            }
            self.sink_input_id = None;
        }

        let now = Instant::now();
        let should_lookup = match self.last_sink_lookup {
            Some(last) => now.duration_since(last) >= LIVE_VOLUME_LOOKUP_THROTTLE,
            None => true,
        };
        if !should_lookup {
            return Ok(false);
        }
        self.last_sink_lookup = Some(now);

        let Some(found_sink_input_id) = find_sink_input_for_pid(child.id())? else {
            return Ok(false);
        };
        self.sink_input_id = Some(found_sink_input_id);

        if set_sink_input_volume(found_sink_input_id, &volume_arg)? {
            self.pending_live_volume = None;
            self.reported_volume_update = Some(target_volume);
            return Ok(true);
        }

        self.sink_input_id = None;
        Ok(false)
    }

    pub fn poll_finished(&mut self) -> Result<Option<ExitStatus>> {
        let Some(_child) = self.child.as_ref() else {
            return Ok(None);
        };

        let exited = self
            .child
            .as_mut()
            .expect("child existence checked")
            .try_wait()
            .context("failed to poll ffplay process")?;

        match exited {
            Some(exited) => {
                self.child = None;
                self.paused = false;
                self.reported_volume_update = None;
                #[cfg(target_os = "linux")]
                {
                    self.pending_live_volume = None;
                    self.sink_input_id = None;
                    self.last_sink_lookup = None;
                }
                if !exited.success() && self.use_fast_start {
                    // If aggressive probe settings fail on a specific environment/codec,
                    // disable fast-start for subsequent tracks.
                    self.use_fast_start = false;
                }
                Ok(Some(exited))
            }
            None => {
                #[cfg(target_os = "linux")]
                {
                    let _ = self.try_apply_pending_live_volume()?;
                }
                Ok(None)
            }
        }
    }

    pub fn stop(&mut self) -> Result<()> {
        if let Some(mut child) = self.child.take() {
            let _ = child.kill();
            let _ = child.wait();
        }
        self.paused = false;
        self.reported_volume_update = None;
        #[cfg(target_os = "linux")]
        {
            self.pending_live_volume = None;
            self.sink_input_id = None;
            self.last_sink_lookup = None;
        }
        Ok(())
    }
}

fn env_flag_enabled(var_name: &str) -> Option<bool> {
    std::env::var(var_name).ok().map(|value| {
        matches!(
            value.trim().to_ascii_lowercase().as_str(),
            "1" | "true" | "yes" | "on"
        )
    })
}

impl Drop for PlaybackEngine {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

fn spawn_ffplay(
    target: &StreamTarget,
    fast_start: bool,
    volume_percent: u8,
    seek_seconds: f64,
) -> Result<Child> {
    let mut command = Command::new("ffplay");
    command.args(["-nodisp", "-autoexit", "-loglevel", "quiet"]);
    #[cfg(target_os = "linux")]
    {
        if std::env::var_os("SDL_AUDIODRIVER").is_none() {
            // Keep ffplay on Pulse-compatible output so live per-stream volume control works.
            command.env("SDL_AUDIODRIVER", "pulseaudio");
        }
    }
    command.args(["-volume", &volume_percent.clamp(0, 100).to_string()]);
    if seek_seconds >= 0.05 {
        command.args(["-ss", &format!("{seek_seconds:.3}")]);
    }
    if fast_start {
        command.args([
            "-fflags",
            "nobuffer",
            "-probesize",
            "32768",
            "-analyzeduration",
            "0",
            "-vn",
            "-sn",
            "-dn",
        ]);
    }

    command
        .arg(&target.url)
        // Keep stdin attached for non-Unix pause/resume control commands.
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .context("unable to spawn ffplay process")
}

#[cfg(not(unix))]
fn send_ffplay_toggle_pause(child: &mut Child) -> Result<()> {
    let stdin = child
        .stdin
        .as_mut()
        .ok_or_else(|| anyhow::anyhow!("ffplay control channel unavailable"))?;
    stdin
        .write_all(b"p")
        .context("failed to send pause command to ffplay")?;
    stdin
        .flush()
        .context("failed to flush pause command to ffplay")?;
    Ok(())
}

#[cfg(target_os = "linux")]
fn set_sink_input_volume(sink_input_id: u32, volume_arg: &str) -> Result<bool> {
    let output = match Command::new("pactl")
        .args([
            "set-sink-input-volume",
            &sink_input_id.to_string(),
            volume_arg,
        ])
        .output()
    {
        Ok(output) => output,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(err) => return Err(err).context("failed to invoke pactl"),
    };

    Ok(output.status.success())
}

#[cfg(target_os = "linux")]
fn detect_live_volume_backend() -> bool {
    command_succeeds("pactl", &["info"])
}

#[cfg(target_os = "linux")]
fn command_succeeds(binary: &str, args: &[&str]) -> bool {
    match Command::new(binary)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
    {
        Ok(status) => status.success(),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => false,
        Err(_) => false,
    }
}

#[cfg(target_os = "linux")]
fn find_sink_input_for_pid(pid: u32) -> Result<Option<u32>> {
    let output = match Command::new("pactl").args(["list", "sink-inputs"]).output() {
        Ok(output) => output,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(err) => return Err(err).context("failed to invoke pactl"),
    };

    if !output.status.success() {
        return Ok(None);
    }

    let text = String::from_utf8_lossy(&output.stdout);
    let mut current_sink_input: Option<u32> = None;
    let mut current_is_ffplay_binary = false;
    let mut current_is_ffplay_name = false;
    let mut ffplay_fallback: Option<u32> = None;

    for raw_line in text.lines() {
        let line = raw_line.trim();
        if let Some(rest) = line.strip_prefix("Sink Input #") {
            if (current_is_ffplay_binary || current_is_ffplay_name) && ffplay_fallback.is_none() {
                ffplay_fallback = current_sink_input;
            }
            current_sink_input = rest.trim().parse::<u32>().ok();
            current_is_ffplay_binary = false;
            current_is_ffplay_name = false;
            continue;
        }

        if let Some(found_pid) = parse_pactl_process_id(line) {
            if found_pid == pid {
                return Ok(current_sink_input);
            }
        } else if let Some(binary) = parse_pactl_property(line, "application.process.binary")
            && binary == "ffplay"
        {
            current_is_ffplay_binary = true;
        } else if let Some(name) = parse_pactl_property(line, "application.name")
            && name.to_ascii_lowercase().contains("ffplay")
        {
            current_is_ffplay_name = true;
        }
    }

    if (current_is_ffplay_binary || current_is_ffplay_name) && ffplay_fallback.is_none() {
        ffplay_fallback = current_sink_input;
    }
    Ok(ffplay_fallback)
}

#[cfg(target_os = "linux")]
fn parse_pactl_property<'a>(line: &'a str, key: &str) -> Option<&'a str> {
    let (found_key, value) = line.split_once('=')?;
    if found_key.trim() != key {
        return None;
    }
    Some(value.trim().trim_matches('"'))
}

#[cfg(target_os = "linux")]
fn parse_pactl_process_id(line: &str) -> Option<u32> {
    parse_pactl_property(line, "application.process.id")?
        .parse::<u32>()
        .ok()
}
