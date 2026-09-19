package org.thisisthepy.dioxus.compose

interface Platform {
    val name: String
}

expect fun getPlatform(): Platform
