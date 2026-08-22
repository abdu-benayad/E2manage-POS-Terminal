# E2Manage POS Terminal - Design System

> **Comprehensive design tokens and component specifications extracted from UI wireframes**
>
> **Version**: 1.0
> **Generated from**: 38 UI wireframe specifications
> **Target**: toolkit-independent — these are values, not widgets

---

## 1. Color Palette

### Primary Colors

| Name | Hex | RGB | Usage |
|------|-----|-----|-------|
| Primary Blue | `#2563EB` | rgb(37, 99, 235) | Buttons, links, focus states, active elements |
| Primary Dark | `#1E3A8A` | rgb(30, 58, 138) | Gradient top, headers, emphasis |
| Primary Light | `#3B82F6` | rgb(59, 130, 246) | Hover states, secondary highlights |

### Semantic Colors

| Name | Hex | Usage |
|------|-----|-------|
| Success | `#22C55E` | Online status, confirmations, balanced state |
| Warning | `#F59E0B` | Offline mode, pending states, variance alerts |
| Danger | `#EF4444` | Errors, void, delete, declined, short variance |
| Info | `#0EA5E9` | Information, face detection outline |

### Background Colors

| Name | Hex/Value | Usage |
|------|-----------|-------|
| Background | `#F8FAFC` | Main screen background |
| Surface | `#FFFFFF` | Cards, panels, inputs |
| Surface Variant | `#F1F5F9` | Secondary surfaces |
| Cart Background | `#FAFAFA` | Cart panel sections |
| Overlay | `rgba(0,0,0,0.5)` | Modal overlays |
| Status Bar BG | `#0B1538` at 90% opacity | Bottom status bar |
| Customer Bar | `#FFF3E0` | Customer indicator bar |

### Gradient

```
Splash Screen Gradient:
- Direction: Top to Bottom
- Top: #1E3A8A
- Bottom: #2563EB
```

### Text Colors

| Name | Value | Usage |
|------|-------|-------|
| Text Primary | `#1E293B` | Main text, headings |
| Text Secondary | `#64748B` | Captions, hints, labels |
| Text On Primary | `#FFFFFF` | Text on blue backgrounds |
| Text Muted | `rgba(255,255,255,0.85)` | Splash screen status text |
| Text On Surface | `rgba(255,255,255,0.9)` | Status bar text |
| Success Text | `#A5D6A7` | Online status text (light green) |
| Orange Accent | `#E65100` | Customer change link |

### Border Colors

| Name | Value | Usage |
|------|-------|-------|
| Border | `#E2E8F0` | Input borders, dividers |
| Border Focus | `#2563EB` | Focused input border |
| Divider | `rgba(0,0,0,0.08)` | Subtle dividers |
| Status Bar Border | `rgba(255,255,255,0.08)` | Optional top border |

### Progress Bar Colors

| Name | Value | Usage |
|------|-------|-------|
| Progress BG | `rgba(255,255,255,0.24)` | Progress bar track |
| Progress Fill | `#FFFFFF` | Progress bar fill |

---

## 2. Typography

### Font Family

- **Primary**: System sans-serif (platform default)
- **Fallback**: Arial, Helvetica, sans-serif
- **Arabic**: Noto Sans Arabic, Arabic UI system fonts

### Font Sizes

| Name | Size | Weight | Line Height | Usage |
|------|------|--------|-------------|-------|
| Display | 32px | 700 | 1.2 | Large totals, emphasis |
| Title | 24px | 600 | 1.3 | App name, screen titles |
| Heading | 20px | 600 | 1.4 | Section headers, tile titles |
| Subheading | 18px | 600 | 1.4 | Card titles, amounts |
| Body | 16px | 400 | 1.5 | Default text, buttons |
| Body Strong | 16px | 600 | 1.5 | Emphasized body text |
| Caption | 14px | 400 | 1.4 | Status bar, hints, secondary |
| Small | 12px | 400 | 1.4 | Labels, badges, timestamps |
| Tiny | 11px | 400 | 1.3 | Category labels |

### Font Weights

| Name | Value | Usage |
|------|-------|-------|
| Regular | 400 | Body text |
| Medium | 500 | Emphasized text |
| Semi-Bold | 600 | Headings, buttons |
| Bold | 700 | Display, strong emphasis |

---

## 3. Spacing Scale

| Name | Value | Usage |
|------|-------|-------|
| 4xs | 2px | Micro adjustments |
| 3xs | 4px | Icon gaps, tight spacing |
| 2xs | 6px | Line spacing |
| xs | 8px | Small gaps, progress bar margin |
| sm | 12px | Component internal padding |
| md | 16px | Standard spacing, gaps |
| lg | 24px | Section spacing |
| xl | 32px | Large gaps, outer margins |
| 2xl | 48px | Screen margins, major sections |

### Common Spacing Patterns

- **Logo to App Name**: 16px
- **App Name to Progress Bar**: 16px
- **Progress Bar to Status Text**: 8px
- **Card Padding**: 16px
- **Input Padding**: 12px horizontal, centered vertical
- **Button Padding**: 16px horizontal, 12px vertical
- **Screen Safe Margin**: 24px (tablet), 32px (desktop)

---

## 4. Border Radius

| Name | Value | Usage |
|------|-------|-------|
| none | 0px | Sharp corners |
| sm | 4px | Small elements, progress bar |
| md | 8px | Buttons, inputs, panels |
| lg | 12px | Cards, main panels |
| xl | 16px | Logo container, major elements |
| 2xl | 20px | Avatar circles |
| full | 9999px | Pills, status dots, chips |

---

## 5. Shadows

| Name | Value | Usage |
|------|-------|-------|
| none | none | Flat elements |
| sm | `0 1px 2px rgba(0,0,0,0.05)` | Subtle lift |
| md | `0 4px 6px rgba(0,0,0,0.1)` | Cards, elevated surfaces |
| lg | `0 10px 15px rgba(0,0,0,0.1)` | Dialogs, modals |
| focus | `0 0 0 2px #2563EB` | Focus ring |

---

## 6. Component Specifications

### Buttons

| Variant | Height | Min Width | Padding | Border Radius | Font |
|---------|--------|-----------|---------|---------------|------|
| Primary | 48px | 120px | 16px 24px | 8px | 16px/600 |
| Primary Large | 56px | 160px | 16px 32px | 8px | 16px/600 |
| Secondary | 44px | 100px | 12px 20px | 8px | 15px/600 |
| Ghost | 40px | 80px | 8px 16px | 8px | 14px/500 |
| Icon | 48px | 48px | 12px | 8px | - |
| Icon Small | 40px | 40px | 8px | 8px | - |
| Shortcut | 120x72px | - | 12px | 8px | 13px/400 |
| Category Tile | 85x85px | - | 8px | 12px | 11px/400 |
| Payment Tile | 220x140px | - | 16px | 8px | 16px/500 |
| Big Option | 420x180px | - | 24px | 8px | 18px/500 |

### Input Fields

| Property | Value |
|----------|-------|
| Height | 44-56px |
| Border | 1px solid #E2E8F0 |
| Border Radius | 8px |
| Padding | 12px 16px |
| Font Size | 16px |
| Focus Border | 2px solid #2563EB |
| Error Border | 2px solid #EF4444 |
| Background | #FFFFFF |

### Cards

| Property | Value |
|----------|-------|
| Background | #FFFFFF |
| Border Radius | 8-12px |
| Shadow | md |
| Padding | 16-24px |

### Status Bar

| Property | Value |
|----------|-------|
| Height | 48px |
| Background | #0B1538 at 90% opacity |
| Font Size | 14px |
| Text Color | rgba(255,255,255,0.9) |
| Padding Horizontal | 24px |
| Padding Vertical | 8px |

### Header Bar

| Property | Value |
|----------|-------|
| Height | 64px |
| Background | Primary color |
| Font Size | 16-20px |
| Text Color | #FFFFFF |
| Padding Horizontal | 16px |

### Progress Bar

| Property | Value |
|----------|-------|
| Height | 8px |
| Width | 300px |
| Border Radius | 4px |
| Track Color | rgba(255,255,255,0.24) |
| Fill Color | #FFFFFF |

### Numeric Keypad

| Property | Desktop | Tablet |
|----------|---------|--------|
| Button Width | 80-96px | 72-84px |
| Button Height | 64-72px | 60-64px |
| Gap | 12-16px | 12px |
| Font Size | 20-24px | 20px |
| Border Radius | 8px | 8px |

### PIN Dots

| Property | Value |
|----------|-------|
| Container Width | 260px |
| Container Height | 44-52px |
| Dot Size | 12px |
| Dot Spacing | 16px |
| Filled Color | #1E293B |
| Empty Color | #E2E8F0 |

### Avatar

| Size | Dimensions | Border Radius |
|------|------------|---------------|
| Small | 40x40px | 20px (full) |
| Medium | 64x64px | 32px (full) |
| Large | 72x72px | 36px (full) |
| XL | 96x96px | 48px (full) |

### List Items

| Property | Value |
|----------|-------|
| Height | 44-56px |
| Padding Horizontal | 12-16px |
| Font Size | 14-16px |
| Border Bottom | 1px solid #E2E8F0 |
| Touch Target | 48px minimum |

### Cashier Tile

| Property | Value |
|----------|-------|
| Min Size | 220x180px |
| Border Radius | 8px |
| Shadow | sm |
| Padding | 16px |
| Avatar Size | 64x64px |
| Name Font | 18-20px semi-bold |
| Role Font | 14-16px muted |

### Status Chips

| Variant | Background | Text Color | Height |
|---------|------------|------------|--------|
| Success | #22C55E | #FFFFFF | 28px |
| Warning | #F59E0B | #FFFFFF | 28px |
| Danger | #EF4444 | #FFFFFF | 28px |
| Neutral | #E2E8F0 | #64748B | 28px |

### Status Dot

| Property | Value |
|----------|-------|
| Size | 8x8px |
| Border Radius | 4px (full) |
| Online Color | #22C55E |
| Offline Color | #F59E0B |
| Error Color | #EF4444 |

---

## 7. Layout Specifications

### Screen Sizes

| Target | Resolution | Safe Area Margin |
|--------|------------|------------------|
| Tablet Portrait | 800x1280 | 16px |
| Tablet Landscape | 1280x800 | 24px |
| Desktop | 1920x1080 | 32px |
| Content Max Width | 1760px (desktop) | - |
| Content Max Width | 1152px (tablet) | - |

### Grid System

| Layout | Left | Right |
|--------|------|-------|
| Checkout Desktop | 55-60% | 40-45% |
| Settings Desktop | 320px nav | flex content |
| Two-Column Modal | 60% | 40% |

### Checkout Layout Ratios

```
Desktop (1920x1080):
- Left Panel (Products/Recent): 55%
- Right Panel (Cart): 45%

Tablet Portrait (800x1280):
- Single column
- Cart as bottom sheet
```

### Settings Navigation

| Property | Value |
|----------|-------|
| Nav Width | 320px |
| Row Height | 44-52px |
| Search Height | 48-56px |
| Active Indicator | 4px left accent |

---

## 8. States

### Interactive States

| State | Modification |
|-------|--------------|
| Default | Base styles |
| Hover | Lighten 10% or subtle shadow increase |
| Pressed | Darken 10%, scale 0.98 |
| Focused | 2px blue outline (#2563EB) |
| Disabled | 50% opacity |
| Loading | Pulse animation or spinner |

### Connection States

| State | Dot Color | Text Label |
|-------|-----------|------------|
| Online | #22C55E | "Online" / "متصل" |
| Offline | #F59E0B | "Offline" / "غير متصل" |
| Error | #EF4444 | "Error" / "خطأ" |
| Syncing | #2563EB | "Syncing..." / "جارٍ المزامنة..." |

### Payment States

| State | Indicator Color | Label |
|-------|-----------------|-------|
| Pending | #F59E0B | "Pending" |
| Processing | #2563EB | "Processing..." |
| Approved | #22C55E | "Approved" |
| Declined | #EF4444 | "Declined" |
| Paid | #22C55E | "PAID" |
| Failed | #EF4444 | "Failed" |

### Shift Variance States

| State | Condition | Color | Label |
|-------|-----------|-------|-------|
| Balanced | variance = 0 | #22C55E | "Balanced" |
| Short | variance < 0 | #EF4444 | "Short" |
| Over | variance > 0 | #F59E0B | "Over" |

---

## 9. RTL Support (Arabic-First)

### Layout Mirroring Rules

| Element | LTR | RTL |
|---------|-----|-----|
| Text Alignment | left | right |
| Flex Direction | row | row-reverse |
| Header App Name | left | right |
| Header Status | right | left |
| Navigation | left | right |
| Progress Fill Direction | left to right | right to left |

### Icon Mirroring

| Icon Type | Mirror in RTL? |
|-----------|----------------|
| Directional (arrows, back) | Yes |
| Symmetric (settings gear) | No |
| Document/content | No |
| Navigation chevrons | Yes |

### Arabic Text Specifications

| Element | Arabic Text |
|---------|-------------|
| App Name | "E2Manage نظام نقاط البيع" |
| Online Status | "متصل" |
| Offline Status | "غير متصل" |
| Syncing | "جارٍ مزامنة المنتجات..." |
| Terminal Label | "الجهاز:" |

### RTL Component Behavior

- Text alignment: right-aligned by default
- Components stack remains vertically centered
- Numeric values: remain right-aligned (universal)
- Header items: reverse order
- Footer buttons: reverse order (primary on left in RTL)
- Lists: text right-aligned, amounts stay right-aligned

---

## 10. Animation Specifications

| Animation | Duration | Easing |
|-----------|----------|--------|
| Button Press | 100ms | ease-out |
| Screen Transition | 200ms | ease-in-out |
| Progress Bar | 300ms | linear |
| Dialog Open | 150ms | ease-out |
| Dialog Close | 100ms | ease-in |
| Toast Show | 200ms | ease-out |
| Toast Hide | 150ms | ease-in |
| Shake (Error) | 300ms | ease-in-out |
| Pulse (Loading) | 1000ms | ease-in-out |
| Face Outline Pulse | 800ms | ease-in-out |

### Transition Properties

```css
/* Standard transition */
transition: all 200ms ease-in-out;

/* Fast interaction */
transition: transform 100ms ease-out, background 100ms ease-out;

/* Dialog */
transition: opacity 150ms ease-out, transform 150ms ease-out;
```

---

## 11. Icons

### Icon Sizes

| Name | Size | Usage |
|------|------|-------|
| XS | 12px | Inline indicators |
| SM | 16px | Small buttons, badges |
| MD | 24px | Standard icons, nav |
| LG | 32px | Featured icons |
| XL | 40px | Payment method icons |
| XXL | 64px | Category tiles |

### Required Icons

| Category | Icons |
|----------|-------|
| Navigation | Menu (hamburger), Back arrow, Close (X), Settings (gear) |
| Status | Online dot, Offline dot, Error dot, Loading spinner |
| Actions | Search, Voice/mic, Camera, Add (+), Remove (-), Edit, Delete |
| Payment | Cash, Card, Mobile/QR, Split, Credit, Gift card |
| Checkout | Cart, Drafts, Transfer, Returns, Void |
| User | Profile/avatar, Face scan, PIN lock |
| Hardware | Printer, Scanner, Cash drawer, EMV terminal |
| Misc | Checkmark, Warning, Info, Refresh |

---

## 12. Touch Targets

### Minimum Touch Target Sizes

| Element | Minimum Size |
|---------|--------------|
| Button | 48x48px |
| Icon Button | 44x44px |
| List Row | 48px height |
| Input Field | 44px height |
| Checkbox/Toggle | 44x44px |
| Dropdown | 44px height |

### Touch Spacing

- Minimum gap between touch targets: 8px
- Recommended gap: 12-16px

---

## 13. Responsive Breakpoints

| Breakpoint | Width | Layout Changes |
|------------|-------|----------------|
| Mobile | < 768px | Single column, bottom sheet cart |
| Tablet Portrait | 768-1024px | Two column, compact spacing |
| Tablet Landscape | 1024-1280px | Standard tablet layout |
| Desktop | > 1280px | Full desktop layout |

### Layout Adaptations

**Tablet Portrait:**
- Navigation: collapsed drawer
- Cart: bottom sheet panel
- Categories: horizontal scroll
- Quick actions: row condensed

**Desktop:**
- Navigation: visible sidebar (settings)
- Cart: right panel
- Categories: full grid
- Quick actions: full row

---

## 14. Accessibility

### Color Contrast Requirements

- Normal text: minimum 4.5:1 ratio
- Large text (18px+): minimum 3:1 ratio
- UI components: minimum 3:1 ratio

### Focus Indicators

- All interactive elements must have visible focus state
- Focus ring: 2px solid #2563EB
- Focus offset: 2px from element

### Touch Target Accessibility

- All touch targets minimum 48x48px
- Labels for icon-only buttons
- Alternative input methods supported

---

## Usage Notes

### Implementation

1. These are tokens, not components. Install them once at the top of the view
   layer rather than passing theme, size or locale per instance.
2. All measurements are in logical pixels; the toolkit handles DPI scaling.
3. The view layer is egui on `abdu-egui-ui`, whose `Environment` is where these
   land. Read that project's conventions before adding a screen.

### RTL Implementation

1. Set `direction: rtl` based on locale
2. Use `horizontal-alignment: start/end` instead of `left/right`
3. Mirror directional icons
4. Test with Arabic text for proper rendering

---

**Document Version**: 1.0
**Last Updated**: 2025-12-12
**Extracted From**: 38 UI wireframe specifications
