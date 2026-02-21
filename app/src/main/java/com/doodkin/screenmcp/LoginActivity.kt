package com.doodkin.screenmcp

import android.content.Intent
import android.os.Bundle
import android.util.Log
import android.view.View
import android.widget.Button
import android.widget.CheckBox
import android.widget.EditText
import android.widget.TextView
import androidx.appcompat.app.AppCompatActivity
import androidx.credentials.CredentialManager
import androidx.credentials.CustomCredential
import androidx.credentials.GetCredentialRequest
import androidx.credentials.GetCredentialResponse
import androidx.lifecycle.lifecycleScope
import com.google.android.libraries.identity.googleid.GetGoogleIdOption
import com.google.android.libraries.identity.googleid.GoogleIdTokenCredential
import com.google.firebase.auth.FirebaseAuth
import com.google.firebase.auth.GoogleAuthProvider
import kotlinx.coroutines.launch

class LoginActivity : AppCompatActivity() {

    private lateinit var auth: FirebaseAuth
    private lateinit var tvStatus: TextView
    private lateinit var cbOpenSourceServer: CheckBox
    private lateinit var etOpenSourceUserId: EditText
    private lateinit var etOpenSourceApiUrl: EditText
    private lateinit var btnOpenSourceContinue: Button

    companion object {
        private const val TAG = "LoginActivity"
        // Web client ID (client_type 3) from google-services.json
        private const val WEB_CLIENT_ID = "979546518393-hrqgk3ebc510pobo6po8eb08qv809gce.apps.googleusercontent.com"
        private const val PREFS_NAME = "screenmcp"
    }

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)

        auth = FirebaseAuth.getInstance()

        val prefs = getSharedPreferences(PREFS_NAME, MODE_PRIVATE)
        val opensourceEnabled = prefs.getBoolean("opensource_server_enabled", false)

        // If open source mode is enabled, skip Firebase check and go straight to main
        if (opensourceEnabled) {
            goToMain()
            return
        }

        // If already signed in via Firebase, skip to main
        if (auth.currentUser != null) {
            goToMain()
            return
        }

        setContentView(R.layout.activity_login)
        tvStatus = findViewById(R.id.tvLoginStatus)
        cbOpenSourceServer = findViewById(R.id.cbOpenSourceServer)
        etOpenSourceUserId = findViewById(R.id.etOpenSourceUserId)
        etOpenSourceApiUrl = findViewById(R.id.etOpenSourceApiUrl)
        btnOpenSourceContinue = findViewById(R.id.btnOpenSourceContinue)

        // Restore saved open source settings
        cbOpenSourceServer.isChecked = prefs.getBoolean("opensource_server_enabled", false)
        etOpenSourceUserId.setText(prefs.getString("opensource_user_id", ""))
        etOpenSourceApiUrl.setText(prefs.getString("opensource_api_url", ""))
        updateOpenSourceFieldsEnabled(cbOpenSourceServer.isChecked)

        findViewById<Button>(R.id.btnGoogleSignIn).setOnClickListener {
            signInWithGoogle()
        }

        cbOpenSourceServer.setOnCheckedChangeListener { _, isChecked ->
            updateOpenSourceFieldsEnabled(isChecked)
            prefs.edit().putBoolean("opensource_server_enabled", isChecked).apply()
        }

        btnOpenSourceContinue.setOnClickListener {
            val userId = etOpenSourceUserId.text.toString().trim()
            val apiUrl = etOpenSourceApiUrl.text.toString().trim()

            if (userId.isEmpty()) {
                tvStatus.text = "Please enter a User ID"
                return@setOnClickListener
            }
            if (apiUrl.isEmpty()) {
                tvStatus.text = "Please enter an API Server URL"
                return@setOnClickListener
            }

            // Save settings
            prefs.edit()
                .putBoolean("opensource_server_enabled", true)
                .putString("opensource_user_id", userId)
                .putString("opensource_api_url", apiUrl)
                .apply()

            goToMain()
        }
    }

    private fun updateOpenSourceFieldsEnabled(enabled: Boolean) {
        etOpenSourceUserId.isEnabled = enabled
        etOpenSourceApiUrl.isEnabled = enabled
        btnOpenSourceContinue.visibility = if (enabled) View.VISIBLE else View.GONE
    }

    private fun signInWithGoogle() {
        tvStatus.text = "Signing in..."

        val googleIdOption = GetGoogleIdOption.Builder()
            .setFilterByAuthorizedAccounts(false)
            .setServerClientId(WEB_CLIENT_ID)
            .build()

        val request = GetCredentialRequest.Builder()
            .addCredentialOption(googleIdOption)
            .build()

        val credentialManager = CredentialManager.create(this)

        lifecycleScope.launch {
            try {
                val result = credentialManager.getCredential(this@LoginActivity, request)
                handleSignInResult(result)
            } catch (e: Exception) {
                Log.e(TAG, "Google sign-in failed", e)
                tvStatus.text = "Sign-in failed: ${e.message}"
            }
        }
    }

    private fun handleSignInResult(result: GetCredentialResponse) {
        val credential = result.credential
        if (credential is CustomCredential &&
            credential.type == GoogleIdTokenCredential.TYPE_GOOGLE_ID_TOKEN_CREDENTIAL
        ) {
            val googleIdTokenCredential = GoogleIdTokenCredential.createFrom(credential.data)
            firebaseAuthWithGoogle(googleIdTokenCredential.idToken)
        } else {
            tvStatus.text = "Unexpected credential type"
        }
    }

    private fun firebaseAuthWithGoogle(idToken: String) {
        val firebaseCredential = GoogleAuthProvider.getCredential(idToken, null)
        auth.signInWithCredential(firebaseCredential)
            .addOnCompleteListener(this) { task ->
                if (task.isSuccessful) {
                    Log.d(TAG, "signInWithCredential:success")
                    goToMain()
                } else {
                    Log.e(TAG, "signInWithCredential:failure", task.exception)
                    tvStatus.text = "Auth failed: ${task.exception?.message}"
                }
            }
    }

    private fun goToMain() {
        startActivity(Intent(this, MainActivity::class.java))
        finish()
    }
}
