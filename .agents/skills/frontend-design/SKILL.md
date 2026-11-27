---
name: frontend-design
description: >-
  Use this skill for building world-class, production-grade frontend interfaces with Next.js,
  React, TypeScript, Tailwind CSS, and shadcn/ui. Enforces strict architectural patterns,
  component reusability, accessibility, responsive ergonomics, and zero-stiffness interactive feel.
---

# Frontend Design Skill

This skill provides expert frontend architecture, implementation guidelines, and component engineering standards for modern web applications.

## Core Directives

1. **Clean Architectural Separation**:
   - `components/ui/`: Atomic, headless or styled primitive components (buttons, dialogs, inputs, tooltips, cards).
   - `components/layout/`: Persistent structural shells (top bar, sidebar, navigation, containers).
   - `components/features/`: Domain-specific feature modules (e.g., `features/dashboard/`, `features/script-editor/`).
   - `hooks/`: Reusable react hooks for queries, keyboard shortcuts, DOM observations.
   - `lib/`: Pure utilities, API clients, formatters, and class-name merging (`cn`).
   - `stores/`: Client state stores (Zustand) with localized state machines and history.

2. **Typography & Hierarchy**:
   - Establish a deliberate typographic scale:
     - `text-2xl font-bold tracking-tight`: Main page titles.
     - `text-lg font-semibold tracking-tight`: Section headings and card titles.
     - `text-sm font-medium`: Field labels, buttons, and navigation links.
     - `text-xs font-medium uppercase tracking-wider text-muted-foreground`: Sub-labels and category trackers.
     - `text-sm leading-relaxed`: Body copy and readable editor inputs.
     - `text-xs font-mono`: Exact timecodes, keyboard hints, word counts.

3. **Color & Surface System**:
   - Solid, high-contrast surfaces over noisy gradients.
   - Neutral backgrounds (`bg-zinc-50` light, `bg-zinc-950` / `bg-[#0F0F12]` dark).
   - Card surfaces with crisp borders (`border-zinc-200` light, `border-zinc-800` dark) and subtle depth.
   - Accent colors applied purposefully to primary calls to action, active tabs, and focus rings.

4. **Interactive Responsiveness & Anti-Stiffness**:
   - All interactive controls must provide immediate visual feedback (`active:scale-[0.98]`, subtle `hover:bg-...`, crisp `focus-visible:ring-2`).
   - Transitions must be swift and purposeful (`transition-colors duration-150`, `ease-out`).
   - Keyboard navigation supported across all modals, tabs, and lists with visible `<kbd>` shortcut indicators.

5. **Strict Constraint Adherence**:
   - Never use gradients where solid contrast is better.
   - Never use full pill radii (`rounded-full`) for buttons or boxes; use `rounded-md` (6px) or `rounded-lg` (8px).
   - Never use emoji icons in UI; use crisp vector SVGs (Lucide icons).
   - Never use vague hero copy or fake statistics; use direct, functional, human copy.

