# Multitool

Logiciel de bureau regroupant plusieurs utilitaires dans une seule interface.
Écrit en Rust avec [Tauri 2](https://tauri.app) : un exécutable léger, compilable pour Windows, Linux et macOS.

> **Règle de maintenance** : ce README est mis à jour à chaque nouvelle tâche ou modification du code.
> Chaque changement ajoute une entrée datée dans le [journal des modifications](#journal-des-modifications),
> et les sections concernées (architecture, utilitaires disponibles, etc.) sont corrigées dans la foulée.

## Sommaire

- [Principe](#principe)
- [Structure du projet](#structure-du-projet)
- [Utilitaires disponibles](#utilitaires-disponibles)
- [Scanner de réseau et scanner de ports](#scanner-de-réseau-et-scanner-de-ports)
- [Ajouter un utilitaire](#ajouter-un-utilitaire)
- [Interface et modes d'affichage](#interface-et-modes-daffichage)
- [Logo](#logo)
- [Compiler et lancer](#compiler-et-lancer)
- [Compilation Windows / Linux / macOS](#compilation-windows--linux--macos)
- [Installateur Windows](#installateur-windows)
- [Tests](#tests)
- [Journal des modifications](#journal-des-modifications)

## Principe

Chaque utilitaire est un fichier Rust indépendant qui **déclare** :

1. ses informations (nom, description, catégorie) ;
2. ses champs de saisie (texte, nombre, liste de choix, case à cocher, dossier) ;
3. ce qu'il fait quand on clique sur « Exécuter » ; s'il est long, il peut aussi signaler son **avancement**
   et l'interface affiche alors une barre de progression.

L'interface graphique n'est pas écrite à la main pour chaque utilitaire : elle se construit toute seule
à partir de ces déclarations. Ajouter un utilitaire ne demande donc **ni modification de l'interface,
ni modification de la configuration de compilation**. Il suffit de déposer un fichier.

```
 ┌─────────────────────────┐   déclare champs + logique
 │ crates/utilities/       │◄───────────────────────────  un fichier par utilitaire
 │   src/modules/*.rs      │
 └───────────┬─────────────┘
             │ découverts automatiquement à la compilation (build.rs)
 ┌───────────▼─────────────┐
 │ crates/core             │  contrat `Utility` + registre `Registry`
 └───────────┬─────────────┘
             │ commandes `list_utilities` et `run_utility` (+ canal de progression)
 ┌───────────▼─────────────┐        ┌────────────────────────────┐
 │ src-tauri               │◄──────►│ ui/  (HTML, CSS, JS)       │
 │ fenêtre + commandes     │  IPC   │ formulaires générés        │
 └─────────────────────────┘        └────────────────────────────┘
```

## Structure du projet

```
multitool/
├── Cargo.toml                  Espace de travail Rust (3 crates)
├── package.json                Outil de compilation Tauri (npm run dev / build)
├── crates/
│   ├── core/                   Contrat des utilitaires et registre (sans interface)
│   └── utilities/
│       ├── build.rs            Découverte automatique des modules
│       └── src/modules/        ← un fichier (ou un dossier) par utilitaire
│           └── video_downloader/   Exemple d'utilitaire en plusieurs fichiers
├── src-tauri/                  Application de bureau (fenêtre, commandes, dialogue de choix de dossier, icônes)
│   ├── tauri.conf.json         Nom, fenêtre, sécurité, formats d'installeurs
│   ├── capabilities/           Permissions de la fenêtre
│   ├── icons/                  Icônes générées depuis le logo (Windows, macOS, Linux)
│   └── installer/              Bandeaux de l'assistant d'installation Windows
├── ui/                         Interface graphique (aucune étape de build)
│   ├── index.html
│   ├── css/style.css           Thèmes clair / sombre / contraste élevé
│   ├── img/logo.svg            Logo affiché en haut à gauche
│   └── js/                     app.js (interface), theme.js (thèmes)
├── logos/                      12 propositions de logo (SVG et PNG) ; le n° 9 est utilisé
├── scripts/                    set-logo.mjs (changer de logo), installer-images.py (bandeaux)
├── .github/workflows/build.yml Compilation Windows, Linux et macOS
└── CLAUDE.md                   Rappel de la règle de mise à jour du README
```

## Utilitaires disponibles

| Identifiant | Nom | Catégorie | Rôle |
|---|---|---|---|
| `text_stats` | Statistiques de texte | Texte | Compte caractères, mots, lignes et phrases |
| `case_converter` | Convertisseur de casse | Texte | MAJUSCULES, minuscules, Titre, snake_case, kebab-case, camelCase, PascalCase |
| `video_downloader` | Téléchargeur de vidéo | Internet | Télécharge une vidéo depuis un lien, avec barre d'avancement (voir ci-dessous) |
| `network_scanner` | Scanner de réseau | Réseau | Liste les machines allumées d'un réseau (`192.168.1.0/24`), avec nom et adresse MAC |
| `port_scanner` | Scanner de ports | Réseau | Teste les ports TCP d'une machine (de quelques ports aux 65 535) et liste les ports ouverts |

Les deux utilitaires de texte servent d'exemples fonctionnels. Ils peuvent être modifiés ou supprimés librement.

## Téléchargeur de vidéo

Formulaire : **lien de la vidéo**, **dossier de destination** (bouton « Parcourir… », dossier Téléchargements proposé la
première fois, puis le dernier dossier utilisé) et **qualité maximale** (utilisée pour les sites vidéo seulement).

Le moteur est choisi automatiquement :

| Lien | Moteur | Prérequis |
|---|---|---|
| Fichier direct (`.mp4`, `.webm`, `.mkv`…) | Téléchargement intégré à Multitool | Aucun |
| Page d'un site vidéo (YouTube, etc.) | [yt-dlp](https://github.com/yt-dlp/yt-dlp) | yt-dlp installé |

**Installer yt-dlp** (seulement pour les pages de sites vidéo) : Windows `winget install yt-dlp`,
macOS `brew install yt-dlp`, Linux `pip install yt-dlp`. Multitool le cherche dans cet ordre : la variable
d'environnement `MULTITOOL_YTDLP`, le dossier de l'application (on peut y déposer `yt-dlp.exe`), puis le `PATH`.
**ffmpeg** est facultatif : s'il est présent (même emplacements), la meilleure qualité est assemblée (image + son
séparés) ; sinon seuls les formats en un seul fichier sont demandés.

Comportement :

- La barre indique le pourcentage, la taille téléchargée, la vitesse et le temps restant. Si le serveur n'annonce pas
  la taille, la barre devient « En cours » sans pourcentage.
- Le fichier est écrit en `.part` puis renommé à la fin : une vidéo incomplète n'a jamais l'air d'une vidéo valide,
  et le `.part` est supprimé en cas d'échec.
- Un fichier existant n'est jamais écrasé : le nouveau devient `video (1).mp4`, `video (2).mp4`…
- Les noms de fichiers sont rendus valides sur Windows, macOS et Linux et ne peuvent pas sortir du dossier choisi.
- Le dossier de destination est créé s'il n'existe pas.
- Lien introuvable (erreur 404 ou 410) : message « Lien introuvable » sans passer par yt-dlp. Autres erreurs du
  serveur (403, 429…) : yt-dlp prend le relais s'il est installé.
- Limite connue : il n'y a pas encore de bouton d'annulation.

Ne téléchargez que des vidéos que vous avez le droit de copier, dans le respect des conditions des sites concernés.

## Scanner de réseau et scanner de ports

Les deux outils vivent dans `crates/utilities/src/modules/network_tools/` (un dossier, deux utilitaires) et fonctionnent
**sans droits administrateur** : ils utilisent le `ping` du système, la table ARP et des connexions TCP ordinaires.
Ils n'envoient donc pas de paquets bruts comme `nmap -sS` ; ils détectent moins de choses, mais n'exigent rien de spécial.

**Scanner de réseau** (équivalent de `nmap -sn 192.168.1.0/24`)

- Champ « Réseau ou adresses » : `192.168.1.0/24` (proposé par défaut pour le réseau de votre machine), plage `192.168.1.10-50`
  ou `192.168.1.10-192.168.1.20`, adresse seule, nom de machine, ou plusieurs éléments séparés par des virgules / espaces.
- Une machine est « active » si elle répond au `ping`, accepte **ou refuse** une connexion sur un port courant
  (80, 443, 22, 445, 139, 135, 3389, 8080 : un refus prouve qu'elle est là), ou figure dans la table ARP.
- Résultat : tableau Adresse IP, Nom (recherche inverse, désactivable), Adresse MAC, Détectée par.
- Limites : 4096 machines par scan (un /20) ; IPv4 uniquement ; les adresses MAC ne sont visibles que sur le réseau local ;
  une machine dont le pare-feu bloque tout peut ne pas apparaître ; la table ARP peut garder quelques machines éteintes depuis peu.

**Scanner de ports**

- Champs : adresse IP ou nom de la machine, ports (`1-1024` par défaut, `22,80,443`, `20-25,80` ou `tous` pour 1 à 65 535),
  délai d'attente par port (600 ms par défaut).
- Verdict par port : ouvert (connexion acceptée), fermé (refus immédiat), sans réponse (pare-feu ou machine éteinte).
  Résultat : récapitulatif (testés, ouverts, fermés, sans réponse, durée) et tableau Port, Service, Aperçu
  (première ligne annoncée par le service, ex. `SSH-2.0-OpenSSH…`).
- Durée : un scan complet des 65 535 ports est instantané sur sa propre machine, mais peut durer plusieurs minutes sur une machine
  qui ne répond pas (délai × ports ÷ 128 connexions simultanées) ; la barre d'avancement montre où l'on en est.
  Il n'y a pas encore de bouton pour annuler un traitement en cours.

**Autorisation** : scanner un réseau ou une machine qui ne vous appartient pas peut être illégal. Pour toute adresse hors réseau privé
(hors 10.x, 172.16-31.x, 192.168.x, 127.x, 169.254.x), la case « J'ai l'autorisation… » doit être cochée, sinon le scan est refusé.

Le tableau des résultats (`Output::with_grid`) s'affiche en pleine largeur sous le formulaire ; « Copier le résultat » le copie
séparé par des tabulations, prêt à coller dans un tableur.

## Ajouter un utilitaire

1. Créer `crates/utilities/src/modules/mon_utilitaire.rs`
   (lettres minuscules, chiffres et `_` ; un nom qui commence par `_` est ignoré).
2. Y écrire le contenu ci-dessous.
3. Compiler : l'utilitaire apparaît dans la liste, dans sa catégorie.

```rust
use multitool_core::{Field, Input, Output, Registry, Utility, UtilityError, UtilityInfo};

struct Doubleur;

impl Utility for Doubleur {
    fn info(&self) -> UtilityInfo {
        // identifiant unique, nom affiché, description, catégorie (regroupement dans la liste)
        UtilityInfo::new("doubleur", "Doubleur", "Multiplie un nombre par deux.", "Calcul")
    }

    fn fields(&self) -> Vec<Field> {
        vec![Field::number("n", "Nombre")]
    }

    fn run(&self, input: &Input) -> Result<Output, UtilityError> {
        let n = input
            .number("n")
            .ok_or_else(|| UtilityError::InvalidInput("Saisissez un nombre.".into()))?;
        Ok(Output::table(vec![("Résultat", (n * 2.0).to_string())]))
    }
}

pub fn register(registry: &mut Registry) {
    registry.register(Box::new(Doubleur));
}
```

Un fichier peut enregistrer plusieurs utilitaires (plusieurs appels à `registry.register`).
Un utilitaire plus gros peut vivre dans un dossier `modules/mon_utilitaire/mod.rs`.

### Types de champs

| Constructeur | Composant affiché | Lecture dans `run` |
|---|---|---|
| `Field::text(id, libellé)` | Ligne de texte | `input.string("id")` |
| `Field::textarea(id, libellé)` | Zone de texte multiligne | `input.string("id")` |
| `Field::number(id, libellé)` | Nombre (`.range(min, max)` pour borner) | `input.number("id")` |
| `Field::select(id, libellé, &[(valeur, texte)])` | Liste de choix | `input.string("id")` |
| `Field::checkbox(id, libellé)` | Case à cocher | `input.bool("id")` |
| `Field::folder(id, libellé)` | Dossier : saisie + bouton « Parcourir… » (Téléchargements par défaut) | `input.string("id")` |

Options chaînables : `.placeholder("…")`, `.default_value(json!(…))`, `.optional()`.

### Types de résultats

- `Output::text("…")` : texte affiché dans l'afficheur (police à chasse fixe).
- `Output::table(vec![("libellé", "valeur"), …])` : lignes libellé / valeur.
- `Grid::new(&["Colonne A", "Colonne B"])` puis `grid.push(vec![…])` et `output.with_grid(grid)` : tableau à colonnes (liste de machines, de ports…). Les lignes restent toujours rectangulaires. Se combine avec `Output::table(…)` et `.with_text(…)`.
- En cas de problème de saisie : `Err(UtilityError::InvalidInput("message clair"))`. Le message est affiché tel quel.

Les utilitaires s'exécutent hors du thread de l'interface : un calcul long ne fige pas la fenêtre.

### Afficher une barre de progression

Un utilitaire long implémente en plus `run_with_progress` (sa méthode `run` peut simplement l'appeler avec `&|_| {}`).
Chaque appel à `progress(...)` met à jour la barre. Sans cette méthode, l'utilitaire fonctionne comme avant, sans barre.

```rust
use multitool_core::ProgressUpdate;

fn run(&self, input: &Input) -> Result<Output, UtilityError> {
    self.run_with_progress(input, &|_| {})
}

fn run_with_progress(
    &self,
    _input: &Input,
    progress: &dyn Fn(ProgressUpdate),
) -> Result<Output, UtilityError> {
    for etape in 1..=10 {
        // … travail …
        progress(ProgressUpdate::new(etape as f64 / 10.0, format!("Étape {etape} sur 10")));
    }
    Ok(Output::text("Terminé"))
}
```

- `ProgressUpdate::new(fraction, message)` : `fraction` va de 0.0 à 1.0 (borné automatiquement).
- `ProgressUpdate::indeterminate(message)` : durée inconnue, barre sans pourcentage.
- Limiter la fréquence des appels (environ 8 par seconde suffisent) : le téléchargeur de vidéo en donne un exemple.

## Interface et modes d'affichage

- **Liste à gauche**, regroupée par catégorie, avec recherche (`Ctrl+K`, `Échap` pour vider).
- **Formulaire à gauche du contenu, afficheur de résultat à droite**, avec un bouton « Copier le résultat ».
  `Ctrl+Entrée` lance l'utilitaire depuis n'importe quel champ.
- **Barre d'avancement** : pendant l'exécution d'un utilitaire qui signale sa progression, l'afficheur montre le
  pourcentage, une barre et un message (taille, vitesse, temps restant). Elle reste lisible dans les quatre
  modes d'affichage (noir sur blanc et blanc sur noir en contraste élevé). Pour les lecteurs d'écran, la barre est
  exposée comme barre de progression ; le début et la fin de l'exécution sont annoncés.
- **Champ dossier** : bouton « Parcourir… » (boîte de dialogue du système) ; le dernier dossier choisi est mémorisé
  pour chaque utilitaire.
- **Thème** : Automatique (suit le système), Clair ou Sombre.
- **Contraste élevé** : bouton en haut à droite, combinable avec le thème clair ou sombre.
  - Clair + contraste : noir sur blanc, bleu profond, traits de 2 px.
  - Sombre + contraste : blanc sur noir, jaune signal, anneau de focus cyan.
  - L'élément sélectionné est plein et l'interrupteur change de position : l'état ne dépend jamais de la seule couleur.
  - Au premier lancement, le contraste suit le réglage du système (`prefers-contrast`) ; le choix est ensuite mémorisé.
- Navigation complète au clavier, focus toujours visible, lien « Aller au contenu », annonces pour lecteurs d'écran.
- Compatible avec le mode « Contraste » de Windows (couleurs forcées).

## Logo

Logo actuel : le **n° 9, « outils croisés »** (clé à fourche et tournevis), `logos/09-outils-croises.svg`.
Il est utilisé pour l'icône du programme (Windows, macOS, Linux), l'installateur et le coin haut gauche de l'interface.

Douze propositions sont rangées dans `logos/` (un SVG modifiable et un PNG chacune). La planche
`logos/planche-des-logos.png` les montre toutes :

| N° | Nom | Idée |
|---|---|---|
| 1 | `01-canif` | Éventail de quatre lames, comme un couteau multifonction |
| 2 | `02-quadrille` | Quatre carrés (premier logo du projet) |
| 3 | `03-ecrou` | Écrou hexagonal et son filetage |
| 4 | `04-monogramme` | Lettre M massive avec un point d'ambre |
| 5 | `05-engrenage` | Engrenage à huit dents |
| 6 | `06-mallette` | Boîte à outils |
| 7 | `07-hub` | Un centre relié à six utilitaires de couleurs différentes |
| 8 | `08-cube` | Cube modulaire en trois faces |
| 9 | `09-outils-croises` | Clé à fourche et tournevis croisés (**retenu**) |
| 10 | `10-tiroirs` | Meuble à trois tiroirs, un par famille d'utilitaires |
| 11 | `11-vis` | Tête de vis à fente |
| 12 | `12-arches` | Un « m » dessiné comme trois poignées |

**Changer de logo** (n'importe quel SVG carré convient, y compris un dessin personnel) :

```bash
npm run logo -- logos/05-engrenage.svg   # icônes de l'application + logo de l'interface
python scripts/installer-images.py       # bandeaux de l'installateur (Python 3 + Pillow)
```

Puis recompiler. `npm run logo` recrée tous les formats d'icônes (`.ico` Windows de 16 à 256 px, `.icns` macOS, PNG).
Le second script s'appuie sur l'icône que le premier vient de produire : toujours les lancer dans cet ordre.

## Compiler et lancer

### Prérequis

- [Rust](https://rustup.rs) (version stable récente)
- [Node.js](https://nodejs.org) 20 ou plus (uniquement pour l'outil `tauri`, l'interface n'a pas d'étape de build)
- **Windows** : WebView2 (déjà présent sur Windows 11) et les outils de compilation C++ de Visual Studio
- **Linux** : `libwebkit2gtk-4.1-dev build-essential libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev`
- **macOS** : les outils en ligne de commande Xcode (`xcode-select --install`)

### Commandes

```bash
npm install        # une seule fois
npm run dev        # lance l'application en mode développement
npm run build      # produit l'exécutable et les installeurs de la plateforme courante
npm test           # tests du noyau et des utilitaires (sans dépendances système)
```

## Compilation Windows / Linux / macOS

Un exécutable ne peut pas se compiler pour un autre système depuis le sien : chaque version est produite
sur son propre système. Le workflow `.github/workflows/build.yml` le fait automatiquement sur GitHub
(Windows en premier, puis Linux et macOS), à chaque envoi de code, sur les trois systèmes en parallèle.

| Système | Fichiers produits |
|---|---|
| Windows | Installateur `Multitool_<version>_x64-setup.exe` (voir [Installateur Windows](#installateur-windows)) |
| Linux | `.AppImage`, `.deb`, `.rpm` |
| macOS | `.dmg` (processeurs Intel et Apple, version universelle) |

Les fichiers se récupèrent dans l'onglet **Actions** du dépôt GitHub, rubrique *Artifacts*
de l'exécution (`multitool-windows`, `multitool-linux`, `multitool-macos`).
On peut aussi lancer la compilation à la main (bouton « Run workflow »).

Pour ne compiler que Windows pour l'instant, supprimer les blocs `linux` et `macos` de la matrice
dans `.github/workflows/build.yml`.

Les versions macOS ne sont pas signées : au premier lancement, clic droit puis « Ouvrir ».
De même, Windows peut afficher un avertissement SmartScreen tant que l'exécutable n'est pas signé.

## Installateur Windows

Le fichier à distribuer est un installateur unique : **`Multitool_0.1.0_x64-setup.exe`** (le numéro suit la
version écrite dans `src-tauri/tauri.conf.json`). Réglages dans `bundle.windows.nsis` de ce fichier :

- assistant d'installation **en français**, sans choix de langue ;
- installation **pour l'utilisateur courant** : aucun droit administrateur n'est demandé ;
- logo du programme et de l'installateur, bandeaux de l'assistant (`src-tauri/installer/sidebar.bmp` et `header.bmp`) ;
- WebView2, le composant d'affichage de l'application, est déjà présent sous Windows 11. Sous Windows 10,
  l'installateur le télécharge si besoin (connexion internet nécessaire à ce moment-là).

**Obtenir le `.exe`** : il doit être compilé sous Windows ; deux façons de faire.

1. **Avec GitHub, sans rien installer sur votre PC.** Envoyer le projet sur un dépôt GitHub, puis onglet *Actions*,
   « Compilation », bouton *Run workflow* : le fichier est dans l'artefact `multitool-windows`. En publiant une
   étiquette de version (`git tag v0.1.0` puis `git push --tags`), le fichier est en plus joint à une
   *version* GitHub, avec un lien de téléchargement direct.
2. **Sur un PC Windows.** Installer [Rust](https://rustup.rs), [Node.js](https://nodejs.org) 20 ou plus et les outils
   de compilation C++ de Visual Studio, puis, dans le dossier du projet :

   ```bash
   npm install
   npm run build -- --bundles nsis
   ```

   Le fichier est produit dans `target\release\bundle\nsis\`.

## Tests

`cargo test` vérifie :

- le registre (exécution, identifiant inconnu, identifiants en double) ;
- chaque utilitaire (cas normaux, accents, saisies invalides) ;
- que tous les utilitaires déclarent des métadonnées valides (identifiant correct, champs sans doublon) ;
- la progression (ordre des mises à jour, valeurs bornées, utilitaires sans progression) et le champ dossier ;
- le téléchargeur de vidéo, contre un serveur HTTP local lancé par les tests (téléchargement et progression,
  redirection, page web, lien introuvable, serveur injoignable, noms de fichiers dangereux, fichiers déjà présents)
  et un faux yt-dlp ; un essai avec le vrai yt-dlp est lancé en plus quand il est installé.
  Aucun accès à internet n'est nécessaire ;
- les scanners : lecture des réseaux (CIDR, plages, listes, limites), des ports et de la table ARP (formats Linux, macOS et Windows),
  détection d'une machine et de ports ouverts / fermés avec bannière contre des serveurs locaux, balayage des 65 535 ports,
  refus des adresses publiques sans autorisation, progression jusqu'à 100 % ;
- le tableau de résultats du noyau (`Grid`, compatibilité avec les anciens résultats).

## Journal des modifications

Une entrée par tâche ou modification, la plus récente en premier.

### 2026-11-24

- **Récupération de l'installateur simplifiée** : le zip de l'artefact `multitool-windows` contenait le `.exe` au fond de sous-dossiers (`release/bundle/nsis/`), difficile à trouver. Le workflow copie maintenant les installateurs dans un dossier à plat : le `.exe` est à la racine du zip.
- **Test complet dans l'application réelle** (pilotée par WebDriver, 5 utilitaires, 4 modes d'affichage) : environ 85 contrôles, tous réussis après les corrections ci-dessous. Couvre la liste et la recherche, les deux scanners (réseau local, plage, adresse publique refusée, saisies invalides, barre de progression qui avance sans reculer, bannière lue, 65 535 ports en 0,4 s en local), le téléchargeur de vidéo (23 contrôles, fichier identique à la source) et le contraste mesuré du tableau dans les 4 modes.
- **Corrigé** : (1) en mode contraste élevé, le bouton « Contraste élevé » devenait illisible quand la souris le survolait (fond écrasé par le survol générique des boutons, texte de la même couleur) ; (2) après un tableau de résultats suivi d'une erreur, la mise en page « tableau pleine largeur » restait active ; (3) le scanner de réseau affiche maintenant un avertissement quand presque toutes les adresses semblent répondre sans aucun ping (un pare-feu ou un proxy répond alors à la place des machines : constaté sur le poste de test, où un réseau inexistant paraissait entièrement actif). 88 tests, clippy et mise en forme propres.

### 2026-11-23

- **Scanner de réseau et scanner de ports** (catégorie Réseau, dossier `network_tools/`) : voir [Scanner de réseau et scanner de ports](#scanner-de-réseau-et-scanner-de-ports). Le noyau gagne `Grid` / `Output::with_grid` (tableau à colonnes) et l'interface l'affiche en pleine largeur, focalisable au clavier, copiable vers un tableur ; vérifié dans les quatre modes d'affichage (une première version coupait les colonnes dans le panneau étroit : le tableau passe maintenant sous le formulaire). Vérifié : 87 tests, clippy et mise en forme propres. Le `ping` n'existe pas sur le poste de développement : le chemin « ping » n'a pas été essayé en réel, la détection par TCP et ARP l'a été.
- **Logo et installateur Windows** : 12 logos dessinés (SVG, PNG et planche de présentation, retouchés après contrôle visuel, lisibilité vérifiée à 64, 32 et 16 px). Le **n° 9 « outils croisés »** est retenu et appliqué : icônes Windows, macOS et Linux (`icon.ico` de 16 à 256 px), coin haut gauche de l'interface, bandeaux de l'assistant d'installation. `npm run logo` change de logo en une commande (essayé : passage au n° 5 puis retour, icônes strictement identiques) ; `src-tauri/app-icon.png`, l'ancienne source d'icône, est supprimé.
- **Installateur** : NSIS configuré en français, installation sans droits administrateur, bandeaux au logo ; le workflow ne produit plus que le `-setup.exe` pour Windows (plus de `.msi` ni de `multitool.exe` seul) et le joint à une version GitHub quand on publie une étiquette `vX.Y.Z`. Vérifié : configuration acceptée par Tauri (une clé inconnue est refusée, donc les clés utilisées sont valides), workflow YAML valide, 54 tests, clippy propre, test complet dans l'application réelle (23 contrôles).
- **Compilation Windows vérifiée** : le projet est sur GitHub (`LecyonM/multitool`) ; la compilation Windows (installateur NSIS `-setup.exe`, 2,7 Mo) et la compilation Linux ont réussi sur GitHub Actions, tests inclus. Récupération de l'installateur : onglet *Actions* du dépôt, exécution la plus récente, artefact `multitool-windows` ; ou, plus simple, publier une version `v0.1.0` (page *Releases* du dépôt) : le workflow y joint le `.exe`. Non essayé : lancement de l'installateur sur un vrai Windows.

### 2026-11-22

- **Téléchargeur de vidéo** (catégorie Internet) : liens directs téléchargés par Multitool, pages de sites vidéo confiées à yt-dlp s'il est installé ; fichier `.part` renommé à la fin, aucun écrasement, noms sûrs, dossier créé au besoin. Vérifié dans l'application réelle (pilotage par WebDriver, serveur local volontairement lent : la barre avance par étapes sans jamais reculer, fichier identique à la source au SHA-256) et avec le vrai yt-dlp sur une page contenant une vidéo. Correction trouvée à ce contrôle : un lien introuvable (404/410) était renvoyé vers yt-dlp avec un message anglais brut, ou suggérait d'installer yt-dlp ; il affiche maintenant « Lien introuvable » en français.
- **Barre d'avancement** : le noyau gagne `ProgressUpdate` et `run_with_progress` (facultatif : les utilitaires existants n'ont pas changé), la commande `run_utility` envoie la progression à l'interface par un canal, et l'afficheur montre pourcentage, barre et message, avec une barre sans pourcentage quand la durée est inconnue. Vérifiée dans les quatre modes d'affichage.
- **Champ dossier** : nouveau type `Field::folder` avec bouton « Parcourir… » (plugin de dialogue Tauri), dossier Téléchargements par défaut, dernier dossier mémorisé par utilitaire.
- **README** : rédaction initiale (principe, structure, guide d'ajout d'un utilitaire, thèmes, compilation) et ajout de `CLAUDE.md` pour rappeler la règle de mise à jour.
- **Compilation multiplateforme** : workflow GitHub Actions en matrice (Windows, Linux, macOS) précédé d'une étape de tests ; jeu d'icônes généré depuis `src-tauri/app-icon.png` ; scripts `npm run dev / build / test`.
- **Interface graphique** : liste d'utilitaires avec recherche, formulaires générés depuis les déclarations Rust, afficheur de résultat avec copie, thèmes clair / sombre / automatique, mode contraste élevé combinable (4 combinaisons), accessibilité clavier et lecteurs d'écran. Correction d'un défaut d'affichage repéré au contrôle visuel : le bouton « Copier le résultat » restait visible en cas d'erreur.
- **Utilitaires d'exemple** : « Statistiques de texte » et « Convertisseur de casse ». Correction du comptage des mots : un signe de ponctuation isolé (« ? », « ! ») n'est plus compté comme un mot.
- **Architecture** : espace de travail Rust en trois crates (`core`, `utilities`, `src-tauri`), contrat `Utility`, registre, découverte automatique des modules par `build.rs` (vérifiée avec un module temporaire, fichier et dossier).
