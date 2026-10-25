# Software Update's pointer

`stable.txt` holds the tag of the release Nagare's Software Update moves copies to. Nagare reads it from the `main`
branch (`https://raw.githubusercontent.com/bungholesurfers/Nagare/main/update/stable.txt`) every day or two, and when
it names a version newer than the copy's own, fetches that release's signed `nagare-update.json`, then its installer.

To roll a release out:

1. Push its tag; the release workflow builds it and drafts the release, with `nagare-update.json` and its signature
   (the workflow needs the secret `NAGARE_UPDATE_KEY` for that).
2. Publish the release, and try it.
3. When you are happy with it, put its tag here (`v0.1.1`, say) and push. Copies with Auto-check on pick it up within
   a day or two.

Setting an older tag holds updates back. It never moves anyone backwards, since Nagare only installs a version newer
than its own.
