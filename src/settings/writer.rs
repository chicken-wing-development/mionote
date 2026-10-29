use super::{Preferences, SettingsStore};
use std::{
    io,
    sync::{Arc, Mutex, mpsc},
    thread::{self, JoinHandle},
};

type Completion = (u64, io::Result<()>);

// Created on the first setting change, so startup and ordinary handwriting do
// not need a settings thread. Only this worker writes settings for this process.
pub struct SettingsWriter {
    sender: Option<mpsc::Sender<(u64, Preferences)>>,
    worker: Option<JoinHandle<()>>,
    completed: Arc<Mutex<Option<Completion>>>,
    revision: u64,
}

impl SettingsWriter {
    pub fn new(store: SettingsStore, notify: impl Fn() + Send + 'static) -> io::Result<Self> {
        let (sender, receiver) = mpsc::channel::<(u64, Preferences)>();
        let completed = Arc::new(Mutex::new(None));
        let result = completed.clone();
        let worker = thread::Builder::new()
            .name("mio-note-settings".into())
            .spawn(move || {
                while let Ok(mut request) = receiver.recv() {
                    // Keep the most recent selection when several clicks arrive
                    // during a slow disk write; never replay obsolete settings.
                    for latest in receiver.try_iter() {
                        request = latest;
                    }
                    let saved = store.save(request.1);
                    if let Ok(mut result) = result.lock() {
                        *result = Some((request.0, saved));
                    }
                    notify();
                }
            })?;
        Ok(Self {
            sender: Some(sender),
            worker: Some(worker),
            completed,
            revision: 0,
        })
    }

    pub fn save(&mut self, preferences: Preferences) -> io::Result<()> {
        let revision = self.revision.wrapping_add(1);
        self.sender
            .as_ref()
            .ok_or_else(|| io::Error::other("Settings writer has stopped"))?
            .send((revision, preferences))
            .map_err(|_| io::Error::other("Settings writer is unavailable"))?;
        self.revision = revision;
        Ok(())
    }

    pub fn take_completion(&self) -> Option<io::Result<()>> {
        let (revision, result) = self.completed.lock().ok()?.take()?;
        (revision == self.revision).then_some(result)
    }

    // Wait only at shutdown. Disconnecting the queue still lets the worker
    // persist its final request, including when the app closes immediately.
    pub fn finish(&mut self) -> io::Result<()> {
        self.sender.take();
        if let Some(worker) = self.worker.take() {
            worker
                .join()
                .map_err(|_| io::Error::other("Settings writer panicked"))?;
        }
        self.take_completion().unwrap_or(Ok(()))
    }
}

impl Drop for SettingsWriter {
    fn drop(&mut self) {
        let _ = self.finish();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn last_selection_survives_immediate_shutdown() {
        let store = super::super::tests::store();
        let mut writer = SettingsWriter::new(store.clone(), || {}).unwrap();
        for index in 0..100 {
            writer
                .save(Preferences {
                    pen: super::super::PenSettings {
                        pressure: index % 2 == 0,
                        ..Default::default()
                    },
                    ..Default::default()
                })
                .unwrap();
        }
        let final_choice = Preferences {
            spacing: super::super::RuleSpacing::Wide,
            ..Default::default()
        };
        writer.save(final_choice).unwrap();
        writer.finish().unwrap();
        assert_eq!(store.load().unwrap(), final_choice);
        super::super::tests::clean(&store);
    }

    #[test]
    fn slow_storage_does_not_block_the_caller_and_failed_writes_are_reported() {
        let store = super::super::tests::store();
        store.save(Preferences::default()).unwrap();
        let locked = super::super::tests::lock(&store);
        let (saved, result) = mpsc::channel();
        let (release, wait) = mpsc::channel();
        let mut writer = SettingsWriter::new(store.clone(), move || {
            saved.send(()).unwrap();
            wait.recv().unwrap();
        })
        .unwrap();
        writer.save(Preferences::default()).unwrap();
        result
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        // The worker is paused. Enqueuing another change must still return.
        writer.save(Preferences::default()).unwrap();
        release.send(()).unwrap();
        result
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        release.send(()).unwrap();
        assert!(writer.finish().is_err());
        drop(locked);
        assert_eq!(store.load().unwrap(), Preferences::default());
        super::super::tests::clean(&store);
    }
}
