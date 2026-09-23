# Moving your notes into Omarchy Notes

Omarchy Notes imports Markdown (`.md`) and plain-text (`.txt`) files, one at a time or as whole
folders. Folders become folders, checklists stay checklists, tables stay tables, and images a
note points to are copied into your library. Anything that can't come across (for example
images hosted on the web) is listed after the import, never silently dropped.

Both options are in the **⋯** menu at the bottom of the folder list:

- **Import Markdown or Text…** for individual files
- **Import a Folder of Notes…** for a whole exported library

## From Apple Notes

Apple Notes can't export to Markdown by itself, so use an exporter on your Mac first:

1. Install **Exporter** (free) from the Mac App Store.
2. Export all folders as **Markdown**. Exporter writes one folder per Notes folder, with
   images alongside.
3. Copy the exported folder to your Omarchy machine.
4. In Omarchy Notes choose **Import a Folder of Notes…** and pick it.

Locked (password-protected) notes can't be exported by any tool; unlock them in Apple Notes
first. Drawings and scanned documents come across as images.

## From Obsidian, Logseq, Bear or any Markdown folder

Choose **Import a Folder of Notes…** and pick your vault or export folder. Front matter at the
top of files is skipped, `#tags` in the text become tags, and GitHub-style task lists
(`- [ ]`) become checklists. Wiki links (`[[Note]]`) come across as plain text.

## From Joplin, Standard Notes and others

Export to Markdown from the app (Joplin: **File → Export all → MD – Markdown**), then import the
folder.

## Leaving again

You're never locked in. Use **Export All Notes as Markdown…** in the same menu to get a folder
of Markdown files that mirrors your folders. Images and attachments are included, and links
between notes become relative links. To move to another machine running Omarchy Notes, use
**Back Up Library…** and **Restore from Backup…** instead: that keeps everything exactly as it
is, including pins, Smart Folders and history.
