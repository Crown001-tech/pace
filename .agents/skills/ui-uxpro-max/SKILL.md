---
name: ui-uxpro-max
description: >-
  Use this skill for senior product design, user experience optimization, layout ergonomics,
  and anti-generic UI craftsmanship. Enforces professional usability standards, visual hierarchy,
  frictionless workflows, and strictly bans gimmicks, AI clichés, and stiff interfaces.
---

# UI/UX Pro Max Skill

This skill governs the visual direction, interaction model, and UX quality standards for the application.

## Product Design Heuristics

1. **Information Architecture & Density**:
   - Every pixel must serve a functional purpose.
   - Group related tools and indicators into contextual toolbars.
   - Never force creators to hunt for essential metrics (word count, estimated time, aspect ratio, save state).
   - Reserve primary accent styling for the single most important action in any view.

2. **Form & Control Design**:
   - Form inputs must have visible, high-contrast borders and clear focus rings.
   - Text inputs and textareas must be comfortable to read and edit with ample line height (`leading-relaxed`).
   - Action buttons must communicate clear intent with verbs (*Create project*, *Save changes*, *Split section*).

3. **Motion & Feedback**:
   - Zero scroll-jacking, zero trailing cursor animations, and zero heavy parallax.
   - Smooth layout transitions when items are added, deleted, or reordered.
   - Immediate optimistic UI feedback with background server synchronization and clear status indicators (`Saving...`, `Saved`, `Sync Error`).

4. **Tone of Voice & Copywriting**:
   - Plain, direct, human language.
   - No buzzwords, no vague marketing statements, no fake metrics or artificial counters.
   - No em dashes in UI text; use periods, commas, colons, or parentheses.
   - Never include "Powered by AI", "Made with AI", or robot iconography. The application is an empowering, professional creative instrument.

5. **Visual Geometry**:
   - No pill-shaped buttons or boxes (`rounded-full`).
   - Use refined, modern corner radii: `rounded-md` (6px) for inputs and buttons; `rounded-lg` (8px) for cards, dialogs, and popovers.
   - Crisp, 1px borders with deliberate contrast ratios.
   - Icons must be monochromatic, refined stroke SVGs (Lucide) sized at 14px to 18px.

