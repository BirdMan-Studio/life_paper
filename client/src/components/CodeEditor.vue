<script setup>
import { onBeforeUnmount, onMounted, ref, watch } from 'vue'
import { Compartment, EditorState } from '@codemirror/state'
import { EditorView, keymap, lineNumbers, highlightActiveLine, highlightActiveLineGutter, drawSelection } from '@codemirror/view'
import { defaultKeymap, history, historyKeymap, indentWithTab } from '@codemirror/commands'
import { bracketMatching, HighlightStyle, indentOnInput, syntaxHighlighting } from '@codemirror/language'
import { python } from '@codemirror/lang-python'
import { tags } from '@lezer/highlight'

const props = defineProps({ modelValue: { type: String, default: '' }, filename: { type: String, default: '' } })
const emit = defineEmits(['update:modelValue', 'save'])
const host = ref(null)
const language = new Compartment()
let editor
const mode = () => /\.py$/i.test(props.filename) ? python() : []
const colors = HighlightStyle.define([
  { tag: tags.keyword, color: '#c5a5f5' },
  { tag: [tags.string, tags.special(tags.string)], color: '#a8d99b' },
  { tag: tags.comment, color: '#82909b', fontStyle: 'italic' },
  { tag: [tags.number, tags.bool, tags.null], color: '#edbc83' },
  { tag: tags.function(tags.variableName), color: '#8fcefa' },
  { tag: [tags.className, tags.typeName], color: '#72d7c5' },
  { tag: tags.operator, color: '#e5d08c' },
  { tag: tags.meta, color: '#e6b0c4' },
])
onMounted(() => {
  editor = new EditorView({
    parent: host.value,
    state: EditorState.create({
      doc: props.modelValue,
      extensions: [
        language.of(mode()), lineNumbers(), history(), drawSelection(),
        highlightActiveLine(), highlightActiveLineGutter(), indentOnInput(), bracketMatching(),
        syntaxHighlighting(colors), EditorState.tabSize.of(4),
        keymap.of([{ key: 'Mod-s', run: () => { emit('save'); return true } }, ...defaultKeymap, ...historyKeymap, indentWithTab]),
        EditorView.contentAttributes.of({ 'aria-label': '代码编辑器', spellcheck: 'false' }),
        EditorView.updateListener.of(update => {
          if (update.docChanged) emit('update:modelValue', update.state.doc.toString())
        }),
        EditorView.theme({
          '&': { height: '100%', backgroundColor: '#0a111b', color: '#c7d5e1' },
          '.cm-scroller': { overflow: 'auto', fontFamily: "'DM Mono', monospace", fontSize: '13px', lineHeight: '1.7' },
          '.cm-content': { padding: '16px 0', caretColor: '#f1f5f9' },
          '.cm-line': { padding: '0 16px' },
          '.cm-gutters': { backgroundColor: '#0a111b', color: '#6f8397', borderRight: '1px solid #213040' },
          '.cm-activeLine, .cm-activeLineGutter': { backgroundColor: '#152331' },
          '.cm-cursor': { borderLeftColor: '#f1f5f9' },
          '&.cm-focused .cm-selectionBackground, .cm-selectionBackground, ::selection': { backgroundColor: '#304960' },
          '&.cm-focused': { outline: 'none' },
        }, { dark: true }),
      ],
    }),
  })
})
watch(() => props.modelValue, value => {
  if (editor && value !== editor.state.doc.toString()) {
    editor.dispatch({ changes: { from: 0, to: editor.state.doc.length, insert: value } })
  }
})
watch(() => props.filename, () => editor?.dispatch({ effects: language.reconfigure(mode()) }))
onBeforeUnmount(() => editor?.destroy())
</script>

<template><div ref="host" class="source-editor"></div></template>

<style scoped>
.source-editor{height:468px;min-height:300px;min-width:0;overflow:hidden}
@media(max-width:600px){.source-editor{height:420px}}
</style>
