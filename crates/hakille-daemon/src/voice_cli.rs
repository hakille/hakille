//! Options et diagnostic de la session vocale CLI. `--listen` reste explicite :
//! aucun micro n'est ouvert par `voice devices` ni pendant le boot ordinaire.
use hakille_perception::audio::{WakeActivation, WakeActivationConfig};
use hakille_voice::input::VoiceInputConfig;
use std::io;
use std::path::PathBuf;

#[derive(Debug, Default)]
pub(crate) struct VoiceOptions {
    pub listen: bool,
    language: Option<String>,
    device_name: Option<String>,
    aliases: Vec<String>,
}

impl VoiceOptions {
    /// Nombre d'arguments consommés, ou None pour une option non vocale.
    pub fn parse_flag(&mut self, args: &[String], index: usize) -> io::Result<Option<usize>> {
        match args[index].as_str() {
            "--listen" => {
                self.listen = true;
                Ok(Some(1))
            }
            flag @ ("--voice-language" | "--voice-device" | "--wake-alias") => {
                let value = args
                    .get(index + 1)
                    .filter(|value| !value.trim().is_empty() && !value.starts_with("--"))
                    .ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!("{flag} exige une valeur"),
                        )
                    })?;
                match flag {
                    "--voice-language" => {
                        let language = value.trim().to_ascii_lowercase();
                        if language != "auto"
                            && !hakille_inference::whisper_engine::WHISPER_LANGUAGES
                                .contains(&language.as_str())
                        {
                            return Err(io::Error::new(
                                io::ErrorKind::InvalidInput,
                                "--voice-language attend auto ou un code de langue Whisper reconnu (fr, en, ar…)",
                            ));
                        }
                        self.language = Some(language);
                    }
                    "--voice-device" => self.device_name = Some(value.clone()),
                    "--wake-alias" => self.aliases.push(value.clone()),
                    _ => unreachable!(),
                }
                Ok(Some(2))
            }
            _ => Ok(None),
        }
    }

    pub fn validate_modes(&self, serve: bool, eval: bool) -> io::Result<()> {
        let error = if self.listen && serve {
            Some("--serve et --listen sont incompatibles : activez le micro du desktop avec F8")
        } else if self.listen && eval {
            Some("--listen est une session continue, incompatible avec --eval-prompt")
        } else if !self.listen
            && (self.language.is_some() || self.device_name.is_some() || !self.aliases.is_empty())
        {
            Some("--voice-language, --voice-device et --wake-alias exigent --listen")
        } else {
            None
        };
        error.map_or(Ok(()), |message| {
            Err(io::Error::new(io::ErrorKind::InvalidInput, message))
        })
    }

    pub fn config(&self, models_dir: PathBuf) -> io::Result<VoiceInputConfig> {
        let mut activation = WakeActivationConfig::default();
        activation.aliases.extend(self.aliases.iter().cloned());
        WakeActivation::new(activation.clone())
            .map_err(|error| io::Error::new(io::ErrorKind::InvalidInput, error))?;
        Ok(VoiceInputConfig {
            models_dir,
            language: self.language.clone().filter(|value| value != "auto"),
            device_name: self.device_name.clone(),
            activation,
        })
    }
}

pub(crate) fn print_devices(args: &[String]) -> Result<(), Box<dyn std::error::Error>> {
    if args != ["devices"] {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Usage : hakille-daemon voice devices",
        )
        .into());
    }
    let devices = hakille_audio::input_device_diagnostics()?;
    println!("Entrées audio annoncées par le système (aucune capture effectuée) :");
    if devices.is_empty() {
        println!("  Aucun périphérique d'entrée énuméré. La capture reste indisponible.");
    }
    for device in devices {
        println!(
            "  {}{}",
            device.name,
            if device.is_default { " [défaut]" } else { "" }
        );
        if let Some(config) = device.config {
            println!(
                "    {} Hz, {} canal/canaux, {}",
                config.sample_rate, config.channels, config.sample_format
            );
        }
        if let Some(error) = device.error {
            println!("    Configuration inutilisable : {error}");
        }
    }
    println!(
        "La permission micro et la réception de samples sont vérifiées au démarrage de --listen."
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(arguments: &[&str]) -> io::Result<VoiceOptions> {
        let arguments: Vec<_> = arguments.iter().map(|s| s.to_string()).collect();
        let mut options = VoiceOptions::default();
        let mut index = 0;
        while index < arguments.len() {
            index += options.parse_flag(&arguments, index)?.unwrap_or(1);
        }
        Ok(options)
    }

    #[test]
    fn voice_flags_are_explicit_and_preserve_device_and_aliases() {
        let options = parse(&[
            "--listen",
            "--voice-language",
            "FR",
            "--voice-device",
            "USB Microphone",
            "--wake-alias",
            "هاكيل",
        ])
        .unwrap();
        options.validate_modes(false, false).unwrap();
        let config = options.config("/models/custom".into()).unwrap();
        assert_eq!(config.models_dir, PathBuf::from("/models/custom"));
        assert_eq!(config.language.as_deref(), Some("fr"));
        assert_eq!(config.device_name.as_deref(), Some("USB Microphone"));
        assert!(
            config
                .activation
                .aliases
                .iter()
                .any(|alias| alias == "هاكيل")
        );
        let auto = parse(&["--listen", "--voice-language", "auto"]).unwrap();
        assert!(auto.config("models".into()).unwrap().language.is_none());
    }

    #[test]
    fn voice_flags_reject_missing_values_and_invalid_aliases() {
        for flag in ["--voice-language", "--voice-device", "--wake-alias"] {
            assert!(parse(&[flag]).is_err());
            assert!(parse(&[flag, "--listen"]).is_err());
            assert!(parse(&[flag, ""]).is_err());
        }
        assert!(parse(&["--listen", "--voice-language", "french"]).is_err());
        assert!(parse(&["--listen", "--voice-language", "zz"]).is_err());
        let options = parse(&["--listen", "--wake-alias", "ha kille"]).unwrap();
        assert!(options.config("models".into()).is_err());
    }

    #[test]
    fn incompatible_voice_modes_fail_before_boot() {
        let options = parse(&["--listen"]).unwrap();
        assert!(
            options
                .validate_modes(true, false)
                .unwrap_err()
                .to_string()
                .contains("F8")
        );
        assert!(options.validate_modes(false, true).is_err());
        assert!(
            parse(&["--voice-language", "auto"])
                .unwrap()
                .validate_modes(false, false)
                .is_err()
        );
        VoiceOptions::default().validate_modes(true, false).unwrap();
    }
}
