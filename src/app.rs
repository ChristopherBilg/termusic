/**
 * MIT License
 *
 * termusic - Copyright (c) 2021 Larry Hao
 *
 * Permission is hereby granted, free of charge, to any person obtaining a copy
 * of this software and associated documentation files (the "Software"), to deal
 * in the Software without restriction, including without limitation the rights
 * to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
 * copies of the Software, and to permit persons to whom the Software is
 * furnished to do so, subject to the following conditions:
 *
 * The above copyright notice and this permission notice shall be included in all
 * copies or substantial portions of the Software.
 *
 * THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
 * IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
 * FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
 * AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
 * LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
 * OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
 * SOFTWARE.
 */
use super::ui::{
    activity::{main::TermusicActivity, Activity, ExitReason},
    context::Context,
};
use crate::config::Termusic;
use crate::player::Player;
use log::error;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread::sleep;
use std::time::Duration;
use std::time::Instant;

pub enum PlayerCommand {
    Play(String),
    TogglePause,
    Seek(i64),
    VolumeUp,
    VolumeDown,
}

pub struct App {
    pub config: Termusic,
    pub quit: bool,           // Becomes true when the user presses <ESC>
    pub redraw: bool,         // Tells whether to refresh the UI; performance optimization
    pub last_redraw: Instant, // Last time the ui has been redrawed
    pub context: Option<Context>,
    player: Player,
    sender: Sender<PlayerCommand>,
    receiver: Receiver<PlayerCommand>,
}

impl App {
    pub fn new(config: Termusic) -> Self {
        let mut ctx: Context = Context::new();
        // Enter alternate screen
        ctx.enter_alternate_screen();
        // Clear screen
        ctx.clear_screen();

        let (tx, rx): (Sender<PlayerCommand>, Receiver<PlayerCommand>) = mpsc::channel();
        Self {
            config,
            quit: false,
            redraw: true,
            last_redraw: Instant::now(),
            context: Some(ctx),
            player: Player::new(),
            sender: tx,
            receiver: rx,
        }
    }

    pub fn run(&mut self) {
        let mut main_activity: TermusicActivity = TermusicActivity::default();
        // Get context
        let ctx: Context = if let Some(ctx) = self.context.take() {
            ctx
        } else {
            error!("Failed to start MainActivity: context is None");
            return;
        };
        // Create activity
        main_activity.init_config(&self.config, self.sender.clone());
        main_activity.on_create(ctx);
        let mut progress_interval = 0;
        loop {
            if let Ok(cmd) = self.receiver.try_recv() {
                match cmd {
                    PlayerCommand::Play(file) => self.player.queue_and_play(&file),
                    PlayerCommand::TogglePause => self.player.pause(),
                    PlayerCommand::Seek(i) => {
                        self.player.seek(i).ok();
                    }
                    PlayerCommand::VolumeUp => self.player.volume_up(),
                    PlayerCommand::VolumeDown => self.player.volume_down(),
                }
            }
            main_activity.update_queue_items();
            main_activity.update_message_box();
            if progress_interval == 0 {
                main_activity.update_progress();
                main_activity.run();
                main_activity.update_download_progress();
                main_activity.update_youtube_search();
            }
            progress_interval += 1;
            if progress_interval >= 8 {
                progress_interval = 0;
            }

            // Draw activity
            main_activity.on_draw();
            // Check if activity has terminated
            if let Some(ExitReason::Quit) = main_activity.will_umount() {
                // info!("SetupActivity terminated due to 'Quit'");
                break;
            }
            // Sleep for ticks
            sleep(Duration::from_millis(20));
        }
        // Destroy activity
        self.context = main_activity.on_destroy();

        drop(self.context.take());
    }
}
