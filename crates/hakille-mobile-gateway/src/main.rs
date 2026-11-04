use hakille_mobile_gateway::{BearerToken, Gateway, GatewayConfig, serve};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

fn usage() {
    println!(
        "hakille-mobile-gateway [--listen 127.0.0.1:17840] [--token-file CHEMIN]\n  [--ipc-endpoint ENDPOINT] [--ipc-token-file CHEMIN]\n  [--request-timeout-secs 680] [--session-idle-secs 900]\n\nOpt-in local : quitter le HUD desktop avant connexion mobile.\nLe jeton mobile est renouvelé au démarrage et n'est jamais affiché.\nUn téléphone distant nécessite un tunnel ou reverse proxy HTTPS explicite."
    );
}

fn destination(path: &Path) -> std::io::Result<PathBuf> {
    let absolute = std::path::absolute(path)?;
    // Canonicaliser aussi un fichier déjà présent : sous Windows, IPC.TOKEN
    // et ipc.token peuvent être le même fichier malgré des chaînes distinctes.
    // Comparer seulement le parent permettrait d'écraser le jeton du démon.
    match std::fs::symlink_metadata(&absolute) {
        Ok(_) => return absolute.canonicalize(),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error),
    }
    let parent = absolute
        .parent()
        .ok_or_else(|| std::io::Error::other("parent de jeton absent"))?;
    std::fs::create_dir_all(parent)?;
    Ok(parent.canonicalize()?.join(
        absolute
            .file_name()
            .ok_or_else(|| std::io::Error::other("nom de fichier de jeton absent"))?,
    ))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut config = GatewayConfig::default();
    let mut token_path = None;
    let mut args = std::env::args().skip(1);
    while let Some(argument) = args.next() {
        if matches!(argument.as_str(), "--help" | "-h") {
            usage();
            return Ok(());
        }
        let value = args.next().ok_or("valeur d'option manquante")?;
        match argument.as_str() {
            "--listen" => config.listen = value.parse().map_err(|_| "adresse invalide")?,
            "--token-file" => token_path = Some(PathBuf::from(value)),
            "--ipc-endpoint" => config.ipc_endpoint = value,
            "--ipc-token-file" => config.ipc_token_file = PathBuf::from(value),
            "--request-timeout-secs" => {
                config.request_timeout =
                    Duration::from_secs(value.parse().map_err(|_| "délai invalide")?)
            }
            "--session-idle-secs" => {
                config.session_idle_timeout =
                    Duration::from_secs(value.parse().map_err(|_| "délai de session invalide")?)
            }
            _ => return Err("option inconnue ; utiliser --help".into()),
        }
    }
    config.validate()?;
    let token_path =
        token_path.unwrap_or_else(|| config.ipc_token_file.with_file_name("mobile-gateway.token"));
    if destination(&token_path)? == destination(&config.ipc_token_file)? {
        return Err("le fichier du jeton mobile doit être distinct du jeton IPC".into());
    }
    // Un deuxième processus échoue avant de remplacer le jeton d'une instance.
    let listener = tokio::net::TcpListener::bind(config.listen).await?;
    let address = listener.local_addr()?;
    let bearer = BearerToken::create_file(&token_path)?;
    let gateway = Arc::new(Gateway::new(config, bearer)?);
    println!("Passerelle mobile locale : http://{address}");
    println!("Fichier privé du jeton mobile : {}", token_path.display());
    println!("Connexion IPC exclusive : le HUD desktop doit être déconnecté.");
    serve(listener, gateway, async {
        let _ = tokio::signal::ctrl_c().await;
    })
    .await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn token_destinations_resolve_existing_aliases_without_rewriting_tokens() {
        let dir =
            std::env::temp_dir().join(format!("gateway-destination-{}", uuid::Uuid::new_v4()));
        std::fs::create_dir_all(&dir).unwrap();
        let ipc = dir.join("ipc.token");
        std::fs::write(&ipc, "jeton de test inchangé").unwrap();
        #[cfg(unix)]
        {
            let alias = dir.join("mobile.token");
            std::os::unix::fs::symlink(&ipc, &alias).unwrap();
            assert_eq!(destination(&ipc).unwrap(), destination(&alias).unwrap());
        }
        #[cfg(windows)]
        {
            let alias = dir.join("IPC.TOKEN");
            if alias.exists() {
                assert_eq!(destination(&ipc).unwrap(), destination(&alias).unwrap());
            } else {
                eprintln!("SKIP : répertoire Windows configuré sensible à la casse");
            }
        }
        assert_eq!(
            std::fs::read_to_string(&ipc).unwrap(),
            "jeton de test inchangé"
        );
        std::fs::remove_dir_all(dir).unwrap();
    }
}
