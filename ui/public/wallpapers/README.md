# Gallery wallpapers
`manifest.json` lists 19 wallpapers (`w01.jpg`..`w19.jpg`, thumbnails in `thumbs/`). The image files are
git-ignored on purpose: this repo is public and the pictures' licenses are unconfirmed (one carries a
third-party site watermark; w05-w09 are only 480x299). Drop the files in this folder to enable the gallery;
the app simply shows an empty gallery if they are missing. Once licensing is confirmed, remove the two
`ui/public/wallpapers` lines from `.gitignore` and commit them.
