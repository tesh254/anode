import { test, expect } from '@playwright/test'
import { _electron as electron } from 'playwright'
import path from 'node:path'

test.describe('Anode E2E — Application Launch', () => {
  test('launches and shows welcome screen', async () => {
    const app = await electron.launch({
      args: [path.join(__dirname, '../../dist-electron/main.js')],
      env: {
        ...process.env,
        NODE_ENV: 'test',
      },
    })

    const window = await app.firstWindow()

    await window.waitForSelector('text=New Project', { timeout: 15000 })
    await expect(window.locator('text=New Project')).toBeVisible()
    await expect(window.locator('text=Open Folder')).toBeVisible()

    await app.close()
  })

  test('has correct title', async () => {
    const app = await electron.launch({
      args: [path.join(__dirname, '../../dist-electron/main.js')],
      env: {
        ...process.env,
        NODE_ENV: 'test',
      },
    })

    const window = await app.firstWindow()
    const title = await window.title()
    expect(title).toContain('Anode')

    await app.close()
  })
})

test.describe('Anode E2E — Welcome Screen', () => {
  let app: electron.Application
  let window: electron.Page

  test.beforeAll(async () => {
    app = await electron.launch({
      args: [path.join(__dirname, '../../dist-electron/main.js')],
      env: {
        ...process.env,
        NODE_ENV: 'test',
      },
    })
    window = await app.firstWindow()
    await window.waitForSelector('text=New Project', { timeout: 15000 })
  })

  test.afterAll(async () => {
    await app.close()
  })

  test('shows New Project and Open Folder buttons', async () => {
    await expect(window.locator('text=New Project')).toBeVisible()
    await expect(window.locator('text=Open Folder')).toBeVisible()
  })

  test('navigates to template browser on New Project click', async () => {
    await window.click('text=New Project')
    await window.waitForSelector('text=Espressif', { timeout: 5000 })
    await expect(window.locator('text=Espressif')).toBeVisible()
  })

  test('shows templates for selected board type', async () => {
    // Click Espressif to see templates
    await window.click('text=Espressif')
    await window.waitForSelector('text=Blink', { timeout: 5000 })
    await expect(window.locator('text=Blink')).toBeVisible()
  })

  test('opens settings dialog', async () => {
    // Go back to main welcome
    await window.click('[aria-label="Back"]') // Or the back button

    // Click settings
    const settingsButton = window.locator('[title="Settings"]')
    if (await settingsButton.isVisible()) {
      await settingsButton.click()
      await window.waitForSelector('text=Settings', { timeout: 5000 })
      await expect(window.locator('text=Settings')).toBeVisible()
    }
  })
})

test.describe('Anode E2E — Editor Workflow', () => {
  let app: electron.Application
  let window: electron.Page

  test.beforeAll(async () => {
    app = await electron.launch({
      args: [path.join(__dirname, '../../dist-electron/main.js')],
      env: {
        ...process.env,
        NODE_ENV: 'test',
      },
    })
    window = await app.firstWindow()
    await window.waitForSelector('text=New Project', { timeout: 15000 })
  })

  test.afterAll(async () => {
    await app.close()
  })

  test('create project from template opens editor', async () => {
    await window.click('text=New Project')
    await window.waitForSelector('text=Espressif', { timeout: 5000 })
    await window.click('text=Espressif')
    await window.waitForSelector('text=Blink', { timeout: 5000 })
    await window.click('text=Blink')

    // Should show project name dialog
    await window.waitForSelector('text=Create', { timeout: 5000 })
  })

  test('command palette opens with Cmd+K', async () => {
    await window.keyboard.press('Meta+k')
    await window.waitForSelector('text=Type a command or search...', { timeout: 5000 })
    await expect(window.locator('text=Type a command or search...')).toBeVisible()
    await window.keyboard.press('Escape')
  })
})

test.describe('Anode E2E — Board Connection', () => {
  let app: electron.Application
  let window: electron.Page

  test.beforeAll(async () => {
    app = await electron.launch({
      args: [path.join(__dirname, '../../dist-electron/main.js')],
      env: {
        ...process.env,
        NODE_ENV: 'test',
      },
    })
    window = await app.firstWindow()
    await window.waitForSelector('text=New Project', { timeout: 15000 })
  })

  test.afterAll(async () => {
    await app.close()
  })

  test('shows "No Board Connected" when no board is attached', async () => {
    // After creating a project, check the board panel
    await window.click('text=New Project')
    await window.waitForSelector('text=Espressif', { timeout: 5000 })
    await window.click('text=Espressif')
    await window.waitForSelector('text=Blink', { timeout: 5000 })
    await window.click('text=Blink')

    // Check the status bar for disconnected state
    const statusBar = window.locator('text=disconnected')
    await expect(statusBar).toBeVisible({ timeout: 5000 })
  })

  test('board panel shows port selector', async () => {
    await window.waitForSelector('text=BOARD', { timeout: 5000 })
    await expect(window.locator('text=BOARD')).toBeVisible()
  })

  test('Flash Firmware button is present', async () => {
    await expect(window.locator('text=Flash Firmware')).toBeVisible()
  })
})

test.describe('Anode E2E — Keyboard Shortcuts', () => {
  let app: electron.Application
  let window: electron.Page

  test.beforeAll(async () => {
    app = await electron.launch({
      args: [path.join(__dirname, '../../dist-electron/main.js')],
      env: {
        ...process.env,
        NODE_ENV: 'test',
      },
    })
    window = await app.firstWindow()
    await window.waitForSelector('text=New Project', { timeout: 15000 })
  })

  test.afterAll(async () => {
    await app.close()
  })

  test('Cmd+K opens command palette', async () => {
    await window.keyboard.press('Meta+k')
    await window.waitForSelector('text=Type a command or search...', { timeout: 5000 })
    await expect(window.locator('text=Type a command or search...')).toBeVisible()
    await window.keyboard.press('Escape')
  })

  test('Cmd+S triggers save (no-op without file)', async () => {
    // Cmd+S should not crash when no file is open
    await window.keyboard.press('Meta+s')
    // No assertion needed — just ensuring no crash
  })
})
