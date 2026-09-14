# Changelog

All notable changes to Insulator. This file is the **source of truth for the release
notes shown in the in-app updater**: [`scripts/release.ts`](scripts/release.ts)
extracts the section whose heading matches the version being released
(`MARKETING_VERSION`) and publishes it next to the update, so Sparkle shows it in
the update prompt.

Format follows [Keep a Changelog](https://keepachangelog.com). Add a new
`## [<version>]` section at the top for each release, matching the version you
set in the Xcode project.

Write release notes for the final product users receive, not the development
history. When a feature is still unreleased, fold its fixes and refinements into
the original feature bullet instead of adding separate entries for them.

## [unreleased]

## [0.1.5]

### Added

- Add a dedicated pull-request workspace with All, Open, Closed, and Merged filters
- Add pull-request search across titles and repositories
- Add pull-request detail views for Summary, Timeline, and Code
- Add branch names, change statistics, reviewers, checks, status, timestamps, and GitHub links to the detail view
- Add pull-request comments with issue and review-comment support, inline file locations, severity tags, collapse/expand controls, and replies
- Add pull-request comment submission with Enter to send and Shift-Enter for a multiline comment
- Add cursor-based pull-request pagination with automatic loading as the list is scrolled
- Add cached GitHub author avatars for pull-request headers

### Changed

- Keep existing pull requests visible while refreshing or loading another page
- Sort pull requests newest first
- Show inline progress and retry feedback when another page is loading or fails
- Refresh pull-request metadata on demand so cached entries receive current branch and comment data
- Make main chat, file, and review tabs and their close controls keyboard-focusable with visible focus states

### Fixed

- Show pull requests opened by other users in repositories owned by the signed-in GitHub user
- Show the correct head and base branches instead of fallback labels
- Show the actual comment count, including review comments
- Show the correct avatar for each pull-request author
- Prevent refresh requests from being lost while pagination is in progress

## [0.1.4]

- Add a Transparent window style with native macOS vibrancy and live window transparency controls
- Keep transparent surfaces, dialogs, pickers, and panels readable as transparency changes
- Keep sidebar and main canvas on the same blur layer, with consistent styling at every transparency level
- Add configurable sidebar transparency and a darker canvas for clearer agent workspaces
- Refresh sidebar, new-task, settings, and supporting UI icons
- Expose the composer to macOS accessibility tools
- Add Intel macOS release builds

## [0.1.3]

- Keep popovers, context menus, and pickers opaque in Liquid Glass and Image window styles for readability
- Fix settings view background opacity in Image and Liquid Glass window styles
- Add file editor autosave and immediate Command-S (`⌘S`) save with toast feedback
- Show Pi subagents in Activity with one row per running agent, start notifications, and Stop/Delete controls
- Fix duplicate subagent entries and garbled ANSI completion toasts
