// CyberManju OS — UseDrag (src/composables/useDrag.ts).
// Role: Vue shared state: transports, wasm backend, vault.
// Exports: DragOptions, DropZoneOptions, useDrag.
// Integrates: vue.
// Contracts: none special (pure logic; errors bubble to caller).
import { onMounted, onUnmounted, type Ref } from 'vue'

export interface DragOptions {
  data?: any
  effect?: 'move' | 'copy' | 'link'
  onStart?: (data: any, e: DragEvent | TouchEvent) => void
  onDrag?: (data: any, e: DragEvent | TouchEvent) => void
  onEnd?: (data: any, e: DragEvent | TouchEvent) => void
  disabled?: boolean | (() => boolean)
}

export interface DropZoneOptions {
  onDrop?: (data: any, e: DragEvent) => void
  onDragOver?: (e: DragEvent) => void
  onDragEnter?: (e: DragEvent) => void
  onDragLeave?: (e: DragEvent) => void
  accept?: (data: any) => boolean
  disabled?: boolean | (() => boolean)
  highlightClass?: string
}

interface DragState {
  active: boolean
  data: any
  ghost: HTMLElement | null
  sourceEl: HTMLElement | null
  startX: number
  startY: number
  touchDragListeners: Array<[string, EventListenerOrEventListenerObject]>
}

const dragState: DragState = {
  active: false,
  data: null,
  ghost: null,
  sourceEl: null,
  startX: 0,
  startY: 0,
  touchDragListeners: [],
}

const DATA_TRANSFER_TYPE = 'application/x-cybermanju-drag'

export function useDrag() {
  function makeDraggable(elRef: Ref<HTMLElement | null>, options: DragOptions) {
    let el: HTMLElement | null = null
    let touchCleanup: (() => void) | null = null

    function isDisabled(): boolean {
      if (typeof options.disabled === 'function') return options.disabled()
      return !!options.disabled
    }

    function handleDragStart(e: DragEvent) {
      if (isDisabled()) return
      if (!e.dataTransfer) return
      const payload = JSON.stringify(options.data)
      e.dataTransfer.setData(DATA_TRANSFER_TYPE, payload)
      e.dataTransfer.effectAllowed = options.effect || 'move'
      dragState.active = true
      dragState.data = options.data
      dragState.sourceEl = el
      options.onStart?.(options.data, e)
    }

    function handleDragEnd(e: DragEvent) {
      if (!dragState.active) return
      dragState.active = false
      dragState.data = null
      dragState.sourceEl = null
      removeGhost()
      options.onEnd?.(options.data, e)
    }

    function handleTouchStart(e: TouchEvent) {
      if (isDisabled() || e.touches.length !== 1) return
      const touch = e.touches[0]
      dragState.startX = touch.clientX
      dragState.startY = touch.clientY
      dragState.data = options.data
      dragState.sourceEl = el
      el!.addEventListener('touchmove', handleTouchMove, { passive: false })
      el!.addEventListener('touchend', handleTouchEnd)
      el!.addEventListener('touchcancel', handleTouchEnd)
      touchCleanup = () => {
        el?.removeEventListener('touchmove', handleTouchMove)
        el?.removeEventListener('touchend', handleTouchEnd)
        el?.removeEventListener('touchcancel', handleTouchEnd)
      }
    }

    function handleTouchMove(e: TouchEvent) {
      e.preventDefault()
      const touch = e.touches[0]
      if (!dragState.ghost && el) {
        createGhost(el, touch.clientX, touch.clientY)
      }
      if (dragState.ghost) {
        dragState.ghost.style.left = touch.clientX - dragState.ghost.offsetWidth / 2 + 'px'
        dragState.ghost.style.top = touch.clientY - dragState.ghost.offsetHeight / 2 + 'px'
      }
      options.onDrag?.(options.data, e)
    }

    function handleTouchEnd(e: TouchEvent) {
      if (dragState.ghost) {
        removeGhost()
        const dropTarget = document.elementFromPoint(
          (e as any).changedTouches?.[0]?.clientX || 0,
          (e as any).changedTouches?.[0]?.clientY || 0
        )
        if (dropTarget) {
          const dropEvent = new CustomEvent('touchdrop', {
            bubbles: true,
            detail: { data: options.data, sourceEl: dragState.sourceEl },
          })
          dropTarget.dispatchEvent(dropEvent)
        }
      }
      dragState.active = false
      dragState.data = null
      options.onEnd?.(options.data, e)
      touchCleanup?.()
    }

    onMounted(() => {
      el = elRef.value
      if (!el) return
      el.draggable = true
      el.addEventListener('dragstart', handleDragStart)
      el.addEventListener('dragend', handleDragEnd)
      el.addEventListener('touchstart', handleTouchStart, { passive: true })
      el.classList.add('cm-draggable')
    })

    onUnmounted(() => {
      if (!el) return
      el.removeEventListener('dragstart', handleDragStart)
      el.removeEventListener('dragend', handleDragEnd)
      el.removeEventListener('touchstart', handleTouchStart)
      touchCleanup?.()
      el.classList.remove('cm-draggable')
    })
  }

  function makeDropZone(elRef: Ref<HTMLElement | null>, options: DropZoneOptions) {
    let el: HTMLElement | null = null
    let dragEnterCount = 0

    function isDisabled(): boolean {
      if (typeof options.disabled === 'function') return options.disabled()
      return !!options.disabled
    }

    function handleDragOver(e: DragEvent) {
      if (isDisabled()) return
      e.preventDefault()
      if (e.dataTransfer) e.dataTransfer.dropEffect = 'move'
      options.onDragOver?.(e)
    }

    function handleDragEnter(e: DragEvent) {
      if (isDisabled()) return
      dragEnterCount++
      if (dragEnterCount === 1) {
        el?.classList.add(options.highlightClass || 'drop-highlight')
      }
      options.onDragEnter?.(e)
    }

    function handleDragLeave(e: DragEvent) {
      if (isDisabled()) return
      dragEnterCount--
      if (dragEnterCount <= 0) {
        dragEnterCount = 0
        el?.classList.remove(options.highlightClass || 'drop-highlight')
      }
      options.onDragLeave?.(e)
    }

    function handleDrop(e: DragEvent) {
      if (isDisabled()) return
      e.preventDefault()
      dragEnterCount = 0
      el?.classList.remove(options.highlightClass || 'drop-highlight')
      const raw = e.dataTransfer?.getData(DATA_TRANSFER_TYPE)
      if (!raw) return
      let data: unknown
      try {
        data = JSON.parse(raw)
      } catch {
        return
      }
      if (options.accept && !options.accept(data)) return
      options.onDrop?.(data, e)
    }

    function handleTouchDrop(e: Event) {
      if (isDisabled()) return
      const ce = e as CustomEvent
      if (options.accept && !options.accept(ce.detail?.data)) return
      const synthetic = new DragEvent('drop', { bubbles: true, cancelable: true })
      options.onDrop?.(ce.detail?.data, synthetic)
      el?.classList.remove(options.highlightClass || 'drop-highlight')
    }

    onMounted(() => {
      el = elRef.value
      if (!el) return
      el.addEventListener('dragover', handleDragOver)
      el.addEventListener('dragenter', handleDragEnter)
      el.addEventListener('dragleave', handleDragLeave)
      el.addEventListener('drop', handleDrop)
      el.addEventListener('touchdrop', handleTouchDrop)
      el.classList.add('cm-drop-zone')
    })

    onUnmounted(() => {
      if (!el) return
      el.removeEventListener('dragover', handleDragOver)
      el.removeEventListener('dragenter', handleDragEnter)
      el.removeEventListener('dragleave', handleDragLeave)
      el.removeEventListener('drop', handleDrop)
      el.removeEventListener('touchdrop', handleTouchDrop)
      el.classList.remove('cm-drop-zone')
    })
  }

  function createGhost(el: HTMLElement, x: number, y: number) {
    const ghost = el.cloneNode(true) as HTMLElement
    ghost.style.position = 'fixed'
    ghost.style.left = x + 'px'
    ghost.style.top = y + 'px'
    ghost.style.pointerEvents = 'none'
    ghost.style.opacity = '0.7'
    ghost.style.zIndex = '9999'
    ghost.style.transform = 'rotate(2deg) scale(1.05)'
    ghost.style.width = el.offsetWidth + 'px'
    ghost.classList.add('cm-drag-ghost')
    document.body.appendChild(ghost)
    dragState.ghost = ghost
  }

  function removeGhost() {
    if (dragState.ghost) {
      dragState.ghost.remove()
      dragState.ghost = null
    }
  }

  function isDragging(): boolean {
    return dragState.active
  }

  function getDragData(): any {
    return dragState.data
  }

  return {
    makeDraggable,
    makeDropZone,
    isDragging,
    getDragData,
  }
}
